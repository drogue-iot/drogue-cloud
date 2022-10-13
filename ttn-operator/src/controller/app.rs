use crate::{
    controller::ensure_stable_app_id,
    data::*,
    ttn::{self},
    utils,
};
use async_trait::async_trait;
use drogue_client::{
    meta::{self, v1::CommonMetadataMut},
    registry, Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{ControllerOperation, ProcessOutcome},
    reconciler::{ReconcileError, ReconcileProcessor, ReconcileState, Reconciler},
};
use headers::{authorization::Credentials, Authorization};
use maplit::{convert_args, hashmap};
use serde_json::{json, Value};
use std::{collections::HashMap, ops::Deref};
use url::Url;

const FINALIZER: &str = "ttn";
const TTN_GATEWAY_NAME: &str = "ttn-gateway";
const TTN_WEBHOOK_NAME: &str = "drogue-iot";

pub struct ApplicationController {
    registry: registry::v1::Client,
    ttn: ttn::Client,
    endpoint_url: Url,
}

impl ApplicationController {
    pub fn new(registry: registry::v1::Client, ttn: ttn::Client, endpoint_url: Url) -> Self {
        Self {
            registry,
            ttn,
            endpoint_url,
        }
    }
}

#[async_trait]
impl ControllerOperation<String, registry::v1::Application, registry::v1::Application>
    for ApplicationController
{
    async fn process_resource(
        &self,
        application: registry::v1::Application,
    ) -> Result<ProcessOutcome<registry::v1::Application>, ReconcileError> {
        ReconcileProcessor(ApplicationReconciler {
            ttn: &self.ttn,
            registry: &self.registry,
            endpoint_url: &self.endpoint_url,
        })
        .reconcile(application)
        .await
    }

    async fn recover(
        &self,
        message: &str,
        mut app: registry::v1::Application,
    ) -> Result<registry::v1::Application, ()> {
        let generation = app.metadata.generation;
        app.update_section(|mut status: TtnAppStatus| {
            status.reconcile = TtnReconcileStatus::failed(generation, message);
            status
        })
        .map_err(|_| ())?;

        Ok(app)
    }
}

impl Deref for ApplicationController {
    type Target = registry::v1::Client;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

pub struct ConstructContext {
    pub app: registry::v1::Application,
    pub spec: TtnAppSpec,
    pub status: Option<TtnAppStatus>,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub spec: Option<TtnAppSpec>,
    pub status: Option<TtnAppStatus>,
}

pub struct ApplicationReconciler<'a> {
    pub ttn: &'a ttn::Client,
    pub registry: &'a registry::v1::Client,
    pub endpoint_url: &'a Url,
}

#[async_trait]
impl<'a> Reconciler for ApplicationReconciler<'a> {
    type Input = registry::v1::Application;
    type Output = registry::v1::Application;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    async fn eval_state(
        &self,
        app: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        let spec = app.section::<TtnAppSpec>().transpose()?;
        let status = app.section::<TtnAppStatus>().and_then(|s| s.ok());

        let requested = spec.is_some();
        let configured = app.metadata.finalizers.iter().any(|f| f == FINALIZER);
        let deleted = app.metadata.deletion_timestamp.is_some();

        Ok(match (requested, configured, deleted) {
            (false, false, _) => {
                // nothing do to
                ReconcileState::Ignore(app)
            }
            (true, _, false) => {
                // we can unwrap here, as we checked before (see 'requested')
                ReconcileState::Construct(ConstructContext {
                    app,
                    spec: spec.unwrap(),
                    status,
                })
            }

            (_, _, true) | (false, true, _) => {
                ReconcileState::Deconstruct(DeconstructContext { app, spec, status })
            }
        })
    }

    async fn construct(
        &self,
        mut ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // ensure

        // ensure we have a finalizer

        if ctx.app.metadata.ensure_finalizer(FINALIZER) {
            // early return
            return Ok(ProcessOutcome::Retry(ctx.app, None));
        }

        // ensure we have a status section, and a stable app id

        let app_id = ctx
            .spec
            .api
            .id
            .as_ref()
            .unwrap_or(&ctx.app.metadata.name)
            .clone();
        let mut status = if let Some(mut status) = ctx.status {
            match status.app_id {
                Some(ref app_id) => ensure_stable_app_id(&ctx.app.metadata, &ctx.spec, app_id)?,
                None => {
                    status.app_id = Some(app_id.clone());
                }
            }
            status
        } else {
            log::debug!("Missing status section, adding...");
            let status = TtnAppStatus {
                reconcile: TtnReconcileStatus {
                    state: "Reconciling".into(),
                    observed_generation: ctx.app.metadata.generation,
                    reason: None,
                },
                app_id: Some(app_id.clone()),
            };
            ctx.app.set_section(status.clone())?;
            status
            // FIXME: return and re-schedule here when we have the work queue
        };

        // ensure the app configuration

        self.ensure_app(&ctx.app, &ctx.spec, &app_id).await?;

        status.reconcile = TtnReconcileStatus::reconciled(ctx.app.metadata.generation);
        ctx.app.set_section(status)?;

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // delete

        if let Some(app_id) = ctx.status.as_ref().and_then(|s| s.app_id.as_ref()) {
            let ttn_ctx = ctx
                .spec
                .ok_or_else(|| ReconcileError::permanent("Missing API configuration."))?
                .api
                .to_context()?;

            self.ttn.delete_app(app_id, &ttn_ctx).await?;
        }

        ctx.app.metadata.remove_finalizer(FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }
}

impl<'a> ApplicationReconciler<'a> {
    async fn ensure_app(
        &self,
        app: &registry::v1::Application,
        spec: &TtnAppSpec,
        ttn_app_id: &str,
    ) -> Result<(), ReconcileError> {
        let ctx = spec.api.to_context()?;
        let gw_password = self
            .ensure_gateway(ttn_app_id, &app.metadata, ctx.clone())
            .await?;

        let ttn_app = self.ttn.get_app(ttn_app_id, &ctx).await?;
        log::debug!("TTN app: {:#?}", ttn_app);
        match ttn_app {
            None => {
                self.ttn
                    .create_app(&app.metadata.name, ttn_app_id, spec.api.owner.clone(), &ctx)
                    .await
            }
            Some(ttn_app) => self.update_app(ttn_app_id, ttn_app, app, &ctx).await,
        }?;

        let auth = Authorization::basic(
            &format!("{}@{}", TTN_GATEWAY_NAME, app.metadata.name),
            &gw_password,
        )
        .0
        .encode();
        let auth = auth
            .to_str()
            .map_err(|_| ReconcileError::permanent("Failed to convert auth information"))?;

        let ttn_webhook = self
            .ttn
            .get_webhook(ttn_app_id, TTN_WEBHOOK_NAME, &ctx)
            .await?;
        match ttn_webhook {
            None => {
                self.ttn
                    .create_webhook(ttn_app_id, TTN_WEBHOOK_NAME, self.endpoint_url, auth, &ctx)
                    .await?;
            }
            Some(ttn_webhook) => {
                if Self::need_webhook_update(ttn_webhook, self.endpoint_url, auth) {
                    self.ttn
                        .update_webhook(ttn_app_id, TTN_WEBHOOK_NAME, self.endpoint_url, auth, &ctx)
                        .await?;
                }
            }
        }

        Ok(())
    }

    fn ensure_gateway_config(
        &self,
        ttn_app_id: &str,
        gateway: &mut registry::v1::Device,
        ctx: ttn::Context,
    ) -> Result<String, ReconcileError> {
        // find a current password

        let password = match gateway.get_credentials() {
            Some(creds) => creds.iter().find_map(|cred| match cred {
                registry::v1::Credential::Password(pwd) => Some(pwd.clone()),
                _ => None,
            }),
            _ => None,
        };

        // if we could not find a password, create one

        let password = if let Some(registry::v1::Password::Plain(password)) = password {
            password
        } else {
            let password = utils::random_password();
            gateway.add_credential(registry::v1::Credential::Password(
                registry::v1::Password::Plain(password.clone()),
            ))?;
            password
        };

        // sync the command endpoint

        let mut headers = HashMap::with_capacity(1);
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", ctx.api_key),
        );

        let mut downlink_url = ctx.url;
        downlink_url
            .path_segments_mut()
            .map_err(|_| ReconcileError::permanent("Unable to modify path"))?
            .extend(&["api", "v3", "as", "applications", ttn_app_id, "devices"]);

        gateway.set_section(registry::v1::DeviceSpecCommands {
            commands: vec![registry::v1::Command::External(
                registry::v1::ExternalCommandEndpoint {
                    r#type: Some("ttnv3".to_string()),
                    url: downlink_url.to_string(),
                    headers,
                    method: String::new(),
                },
            )],
        })?;

        // done

        Ok(password)
    }

    /// Ensure that we have a gateway device for connecting the TTN webhook to.
    ///
    /// This will return a password, which can be used as the gateway password.
    async fn ensure_gateway(
        &self,
        app_id: &str,
        metadata: &meta::v1::NonScopedMetadata,
        ctx: ttn::Context,
    ) -> Result<String, ReconcileError> {
        let gateway = self
            .registry
            .get_device(&metadata.name, TTN_GATEWAY_NAME)
            .await
            .map_err(ReconcileError::temporary)?;

        log::debug!("Retrieved TTN gateway device: {:#?}", gateway);

        let password = match gateway {
            None => {
                log::debug!("Creating new gateway");

                let mut gateway = registry::v1::Device {
                    metadata: meta::v1::ScopedMetadata {
                        application: metadata.name.clone(),
                        name: TTN_GATEWAY_NAME.into(),
                        labels: convert_args!(hashmap!(
                            "ttn/app-id" => app_id,
                        )),
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let password = self.ensure_gateway_config(app_id, &mut gateway, ctx)?;

                self.registry
                    .create_device(&gateway)
                    .await
                    .map_err(ReconcileError::temporary)?;

                password
            }
            Some(mut gateway) => {
                log::debug!("Updating existing gateway");

                let password = self.ensure_gateway_config(app_id, &mut gateway, ctx)?;

                self.registry
                    .update_device(&gateway)
                    .await
                    .map_err(ReconcileError::temporary)?;

                password
            }
        };

        Ok(password)
    }

    fn need_webhook_update(current: Value, url: &Url, auth: &str) -> bool {
        let mut expected = current.clone();

        expected["base_url"] = json!(url);
        expected["format"] = json!("json");
        expected["headers"]["Authorization"] = json!(auth);
        expected["uplink"] = json!({});

        log::debug!("Current: {:#?}", current);
        log::debug!("Expected: {:#?}", expected);

        expected != current
    }

    async fn update_app(
        &self,
        app_id: &str,
        mut ttn_app: Value,
        app: &registry::v1::Application,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        let original = ttn_app.clone();

        ttn_app["name"] = json!(app.metadata.name);
        ttn_app["attributes"]["drogue-app"] = json!(app.metadata.name);

        if original != ttn_app {
            log::debug!("Updating application in TTN");
            self.ttn.update_app(&app.metadata.name, app_id, ctx).await?;
        }

        Ok(())
    }
}
