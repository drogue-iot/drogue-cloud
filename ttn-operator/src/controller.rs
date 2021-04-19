use crate::{
    data::{TtnAppSpec, TtnAppStatus},
    error::ReconcileError,
    ttn::{self, Owner},
};
use actix_http::http::header::IntoHeaderValue;
use actix_web_httpauth::headers::authorization::Basic;
use drogue_client::meta::v1::NonScopedMetadata;
use drogue_client::registry::v1::{Credential, DeviceSpecCredentials};
use drogue_client::{meta, registry, Translator};
use serde_json::Value;
use url::Url;

pub struct Controller {
    registry: registry::v1::Client,
    ttn: ttn::Client,
    endpoint_url: Url,
}

impl Controller {
    pub fn new(registry: registry::v1::Client, ttn: ttn::Client, endpoint_url: Url) -> Self {
        Self {
            registry,
            ttn,
            endpoint_url,
        }
    }

    fn failed(
        mut app: registry::v1::Application,
        err: ReconcileError,
    ) -> Result<registry::v1::Application, anyhow::Error> {
        let generation = app.metadata.generation;
        app.update_section(|mut status: TtnAppStatus| {
            status.state = "Failed".into();
            status.reason = Some(err.to_string());
            if matches!(err, ReconcileError::Permanent(_)) {
                status.observed_generation = generation;
            }
            status
        })?;
        Ok(app)
    }

    pub async fn handle_event(&self, app: String) -> Result<(), anyhow::Error> {
        log::info!("Application changed: {:#?}", app);

        let app = self.registry.get_app(&app, Default::default()).await?;
        log::debug!("Reconcile application: {:#?}", app);

        if let Some(app) = app {
            let app = self.reconcile_app(app.clone()).await.or_else(|err| {
                log::info!("Failed to reconcile: {}", err);
                Self::failed(app, err)
            })?;
            log::debug!("Storing: {:#?}", app);
            self.registry.update_app(app, Default::default()).await?;
        }

        Ok(())
    }

    /// ensures that the finalizer is set
    ///
    /// Returns `true` if the finalizer was added and the resource must be stored
    fn ensure_finalizer(&self, app: &mut registry::v1::Application) -> bool {
        if !app.metadata.finalizers.iter().any(|r| r == "ttn") {
            app.metadata.finalizers.push("ttn".into());
            true
        } else {
            false
        }
    }

    async fn reconcile_app(
        &self,
        mut app: registry::v1::Application,
    ) -> Result<registry::v1::Application, ReconcileError> {
        let spec = app.section::<TtnAppSpec>().transpose()?;
        let mut status = app
            .section::<TtnAppStatus>()
            .transpose()?
            .unwrap_or_default();

        let spec = if let Some(spec) = spec {
            spec
        } else {
            return Err(ReconcileError::permanent(
                "Previously managed TTN application, but TTN API information is now missing",
            ));
        };

        let ctx = ttn::Context {
            api_key: spec.api.api_key.clone(),
            url: spec.api.region.url().map_err(ReconcileError::permanent)?,
        };

        if app.metadata.deletion_timestamp.is_none() {
            // only add the finalizer is we are not yet deleted
            if self.ensure_finalizer(&mut app) {
                // return early to store the finalizer
                return Ok(app);
            }
        } else {
            // if we are, take care of destruction
            self.delete_app(&mut app.metadata, &spec, &mut status, &ctx)
                .await?;
            app.set_section(status)?;

            // return early as we are done here
            return Ok(app);
        }

        let app_id = spec.api.id.as_ref().unwrap_or(&app.metadata.name).clone();
        if let Some(ref existing_app_id) = status.app_id {
            if existing_app_id != &app_id {
                return Err(ReconcileError::permanent(format!(
                    "Application IDs have changed - requested: {}, existing: {}",
                    app_id, existing_app_id
                )));
            }
        }

        self.ensure_app(&app_id, &app.metadata, &spec, &mut status, &ctx)
            .await?;

        status.observed_generation = app.metadata.generation;
        status.state = "Reconciled".into();
        status.reason = None;
        status.app_id = Some(app_id);

        app.set_section(status)?;

        Ok(app)
    }

    async fn ensure_app(
        &self,
        app_id: &str,
        metadata: &NonScopedMetadata,
        spec: &TtnAppSpec,
        status: &TtnAppStatus,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        let gateway = self
            .registry
            .get_device(&metadata.name, "ttn-gateway", Default::default())
            .await
            .map_err(ReconcileError::temporary)?;

        let password = match gateway {
            None => {
                let mut gateway = registry::v1::Device {
                    metadata: meta::v1::ScopedMetadata {
                        application: metadata.name.clone(),
                        name: "ttn-gateway".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let password = "hey-rodney".to_string();
                gateway.update_section(|mut credentials: DeviceSpecCredentials| {
                    credentials.credentials = vec![Credential::Password(password.clone())];
                    credentials
                })?;
                self.registry
                    .create_device(gateway, Default::default())
                    .await
                    .map_err(ReconcileError::temporary)?;
                password
            }
            Some(mut gateway) => {
                let password = "hey-rodney".to_string();
                gateway.update_section(|mut credentials: DeviceSpecCredentials| {
                    // FIXME: we should re-use the existing password
                    credentials.credentials = vec![Credential::Password(password.clone())];
                    credentials
                })?;
                self.registry
                    .update_device(gateway, Default::default())
                    .await
                    .map_err(ReconcileError::temporary)?;
                password
            }
        };

        let ttn_app = self.ttn.get_app(app_id, &ctx).await?;
        log::debug!("TTN app: {:#?}", ttn_app);
        match ttn_app {
            None => {
                self.create_app(app_id, Owner::User(spec.api.owner.clone()), &ctx)
                    .await
            }
            Some(ttn_app) => self.update_app(app_id, ttn_app, &ctx).await,
        }?;

        // let creds = Credential::Password("hey-rodney".into());
        let auth = Basic::new("ttn-gateway", Some(password))
            .try_into_value()
            .map_err(|_| ReconcileError::permanent("Failed to convert auth information"))?;
        let auth = auth
            .to_str()
            .map_err(|_| ReconcileError::permanent("Failed to convert auth information"))?;

        let ttn_webhook = self.ttn.get_webhook(app_id, "drogue-iot", &ctx).await?;
        match ttn_webhook {
            None => {
                self.ttn
                    .create_webhook(app_id, "drogue-iot", &self.endpoint_url, auth, &ctx)
                    .await?;
            }
            Some(ttn_webhook) => {
                // FIXME: update and diff
            }
        }

        Ok(())
    }

    async fn create_app(
        &self,
        app_id: &str,
        owner: Owner,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        log::debug!("Creating TTN app: {}", app_id);

        self.ttn.create_app(app_id, owner, &ctx).await?;

        Ok(())
    }

    async fn update_app(
        &self,
        app_id: &str,
        ttn_app: Value,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        Ok(())
    }

    async fn delete_app(
        &self,
        metadata: &mut NonScopedMetadata,
        _: &TtnAppSpec,
        status: &mut TtnAppStatus,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        if let Some(app_id) = &status.app_id {
            self.ttn.delete_app(app_id, &ctx).await?;
            status.app_id = None;
        }

        metadata.finalizers.retain(|f| f != "ttn");

        Ok(())
    }
}
