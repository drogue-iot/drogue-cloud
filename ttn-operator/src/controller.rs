use crate::{
    data::{TtnAppSpec, TtnAppStatus, TtnDeviceSpec, TtnDeviceStatus, TtnReconcileStatus},
    error::ReconcileError,
    ttn::{self, Owner},
    utils,
};
use actix_http::http::header::IntoHeaderValue;
use actix_web_httpauth::headers::authorization::Basic;
use drogue_client::{
    meta::{self, v1::CommonMetadataMut},
    registry, Translator,
};
use maplit::{convert_args, hashmap};
use serde_json::{json, Value};
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

    fn failed(generation: u64, err: ReconcileError) -> TtnReconcileStatus {
        TtnReconcileStatus {
            observed_generation: generation,
            state: "Failed".into(),
            reason: Some(err.to_string()),
        }
    }

    pub async fn handle_device_event(
        &self,
        app_id: String,
        device_id: String,
    ) -> Result<(), anyhow::Error> {
        log::info!("Device changed: {} / {}", app_id, device_id);

        let app = self.registry.get_app(&app_id, Default::default()).await?;
        let device = self
            .registry
            .get_device(&app_id, &device_id, Default::default())
            .await?;
        log::debug!("Reconcile device: {:#?}", device);

        match (app, device) {
            (Some(app), Some(mut device)) => {
                let device = self
                    .reconcile_device(app, device.clone())
                    .await
                    .or_else::<ReconcileError, _>(|err| {
                        log::info!("Failed to reconcile: {}", err);
                        let generation = device.metadata.generation;
                        device.update_section(|mut status: TtnDeviceStatus| {
                            status.reconcile = Self::failed(generation, err);
                            status
                        })?;

                        Ok(device)
                    })?;
                log::debug!("Storing: {:#?}", device);
                self.registry
                    .update_device(device, Default::default())
                    .await?;
            }
            _ => {
                // If application and/or device are missing, we have nothing to do. As we have
                // finalizers to guard against this.
                // If any of the resources is gone, we can ignore this.
            }
        }

        Ok(())
    }

    async fn reconcile_device(
        &self,
        app: registry::v1::Application,
        mut device: registry::v1::Device,
    ) -> Result<registry::v1::Device, ReconcileError> {
        let app_spec = app.section::<TtnAppSpec>().transpose()?;
        let app_status = app.section::<TtnAppStatus>().transpose()?;
        let device_spec = device.section::<TtnDeviceSpec>().transpose()?;

        let requested = device_spec.is_some();
        let configured = device.metadata.finalizers.iter().any(|f| f == "ttn");
        let deleted = device.metadata.deletion_timestamp.is_some();

        log::debug!(
            "State - requested: {}, configured: {}, deleted: {}",
            requested,
            configured,
            deleted
        );

        match (requested, configured, deleted) {
            (false, false, false) => {
                // nothing do to
                return Ok(device);
            }
            (true, false, false) => {
                if Self::ensure_finalizer(&mut device.metadata) {
                    // early return
                    return Ok(device);
                }
            }
            (true, true, false) => {
                // we can unwrap here, as we checked before (see 'requested')
                let device_spec = device_spec.unwrap();

                // ensure

                // ensure we have a status section, and a stable app id

                let app_spec = app_spec.ok_or_else(|| {
                    ReconcileError::permanent(
                        "Missing TTN configuration in application. Unable to process.",
                    )
                })?;
                let app_status = app_status.ok_or_else(|| {
                    ReconcileError::temporary(
                        "Missing TTN status information in application. Waiting ...",
                    )
                })?;

                let app_id = app_status
                    .app_id
                    .as_ref()
                    .unwrap_or(&app.metadata.name)
                    .clone();

                Self::ensure_stable_app_id(&app.metadata, &app_spec, &app_id)?;

                // ensure the device configuration

                self.ensure_device(&device.metadata, &app_spec, &app_id, &device_spec)
                    .await?;
            }

            (_, _, true) | (false, true, _) => {
                // delete

                if let Some(app_id) = app_status.as_ref().and_then(|s| s.app_id.as_ref()) {
                    let ctx = app_spec
                        .ok_or_else(|| {
                            ReconcileError::permanent("Missing API configuration in application.")
                        })?
                        .api
                        .to_context()?;

                    let device_id = &device.metadata.name;

                    self.ttn.delete_device(&app_id, &device_id, &ctx).await?;
                }

                device.metadata.finalizers.retain(|f| f != "ttn");
            }

            _ => {
                // invalid state
                return Err(ReconcileError::permanent(format!(
                    "Invalid state - requested: {}, configured: {}, deleted: {}",
                    requested, configured, deleted
                )));
            }
        };

        Ok(device)
    }

    pub async fn handle_app_event(&self, app: String) -> Result<(), anyhow::Error> {
        log::info!("Application changed: {}", app);

        let app = self.registry.get_app(&app, Default::default()).await?;
        log::debug!("Reconcile application: {:#?}", app);

        if let Some(mut app) = app {
            let app = self
                .reconcile_app(app.clone())
                .await
                .or_else::<ReconcileError, _>(|err| {
                    log::info!("Failed to reconcile: {}", err);
                    let generation = app.metadata.generation;
                    app.update_section(|mut status: TtnAppStatus| {
                        status.reconcile = Self::failed(generation, err);
                        status
                    })?;

                    Ok(app)
                })?;
            log::debug!("Storing: {:#?}", app);
            self.registry.update_app(app, Default::default()).await?;
        }

        Ok(())
    }

    /// ensures that the finalizer is set
    ///
    /// Returns `true` if the finalizer was added and the resource must be stored
    fn ensure_finalizer(meta: &mut dyn CommonMetadataMut) -> bool {
        if !meta.finalizers().iter().any(|r| r == "ttn") {
            let mut finalizers = meta.finalizers().clone();
            finalizers.push("ttn".into());
            meta.set_finalizers(finalizers);
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
        let status = app.section::<TtnAppStatus>().and_then(|s| s.ok());

        let requested = spec.is_some();
        let configured = status.is_some();
        let deleted = app.metadata.deletion_timestamp.is_some();

        match (requested, configured, deleted) {
            (false, false, false) => {
                // nothing do to
                return Ok(app);
            }
            (true, true, false) => {
                // we can unwrap here, as we checked before (see 'requested')
                let spec = spec.unwrap();

                // ensure

                // ensure we have a finalizer
                if Self::ensure_finalizer(&mut app.metadata) {
                    // early return
                    return Ok(app);
                }

                // ensure we have a status section, and a stable app id

                let app_id = spec.api.id.as_ref().unwrap_or(&app.metadata.name).clone();
                if let Some(status) = status {
                    Self::ensure_stable_app_id(&app.metadata, &spec, &app_id)?;
                    status
                } else {
                    log::debug!("Missing status section, adding...");
                    let status = TtnAppStatus {
                        reconcile: TtnReconcileStatus {
                            state: "Reconciling".into(),
                            observed_generation: app.metadata.generation,
                            reason: None,
                        },
                        app_id: Some(app_id),
                    };
                    app.set_section(status)?;
                    // early return
                    return Ok(app);
                };

                // ensure the app configuration

                self.ensure_app(&app.metadata, &spec, &app_id).await?;
            }

            (_, _, true) | (false, true, _) => {
                // delete

                if let Some(app_id) = status.as_ref().and_then(|s| s.app_id.as_ref()) {
                    let ctx = spec
                        .ok_or_else(|| ReconcileError::permanent("Missing API configuration."))?
                        .api
                        .to_context()?;

                    self.ttn.delete_app(&app_id, &ctx).await?;
                }

                app.metadata.finalizers.retain(|f| f != "ttn");
            }

            _ => {
                // invalid state
                return Err(ReconcileError::permanent(format!(
                    "Invalid state - requested: {}, configured: {}, deleted: {}",
                    requested, configured, deleted
                )));
            }
        };

        Ok(app)
    }

    /// Ensure that the app ID did not change.
    fn ensure_stable_app_id(
        meta: &meta::v1::NonScopedMetadata,
        spec: &TtnAppSpec,
        current_app_id: &str,
    ) -> Result<(), ReconcileError> {
        let defined_id = spec.api.id.as_ref().unwrap_or(&meta.name);
        if &defined_id != &current_app_id {
            Err(ReconcileError::permanent(format!(
                "Application IDs have changed - requested: {}, current: {}",
                defined_id, current_app_id
            )))
        } else {
            Ok(())
        }
    }

    async fn ensure_app(
        &self,
        metadata: &meta::v1::NonScopedMetadata,
        spec: &TtnAppSpec,
        ttn_app_id: &str,
    ) -> Result<(), ReconcileError> {
        let ctx = spec.api.to_context()?;
        let gw_password = self.ensure_gateway(ttn_app_id, &metadata).await?;

        let ttn_app = self.ttn.get_app(ttn_app_id, &ctx).await?;
        log::debug!("TTN app: {:#?}", ttn_app);
        match ttn_app {
            None => {
                self.ttn
                    .create_app(ttn_app_id, Owner::User(spec.api.owner.clone()), &ctx)
                    .await
            }
            Some(ttn_app) => self.update_app(ttn_app_id, ttn_app, &ctx).await,
        }?;

        let auth = Basic::new("ttn-gateway", Some(gw_password))
            .try_into_value()
            .map_err(|_| ReconcileError::permanent("Failed to convert auth information"))?;
        let auth = auth
            .to_str()
            .map_err(|_| ReconcileError::permanent("Failed to convert auth information"))?;

        let ttn_webhook = self.ttn.get_webhook(ttn_app_id, "drogue-iot", &ctx).await?;
        match ttn_webhook {
            None => {
                self.ttn
                    .create_webhook(ttn_app_id, "drogue-iot", &self.endpoint_url, auth, &ctx)
                    .await?;
            }
            Some(ttn_webhook) => {
                if Self::need_webhook_update(ttn_webhook, &self.endpoint_url, auth) {
                    self.ttn
                        .update_webhook(ttn_app_id, "drogue-iot", &self.endpoint_url, auth, &ctx)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Ensure that we have a gateway device for connecting the TTN webhook to.
    ///
    /// This will return a password, which can be used as the gateway password.
    async fn ensure_gateway(
        &self,
        app_id: &str,
        metadata: &meta::v1::NonScopedMetadata,
    ) -> Result<String, ReconcileError> {
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
                        labels: convert_args!(hashmap!(
                            "ttn/app-id" => app_id,
                        )),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let password = utils::random_password();
                gateway.update_section(
                    |mut credentials: registry::v1::DeviceSpecCredentials| {
                        credentials.credentials =
                            vec![registry::v1::Credential::Password(password.clone())];
                        credentials
                    },
                )?;
                self.registry
                    .create_device(gateway, Default::default())
                    .await
                    .map_err(ReconcileError::temporary)?;
                password
            }
            Some(mut gateway) => {
                // find a current password

                let password = match gateway.section::<registry::v1::DeviceSpecCredentials>() {
                    Some(Ok(creds)) => {
                        if let Some(password) = creds.credentials.iter().find_map(|cred| match cred
                        {
                            registry::v1::Credential::Password(pwd) => Some(pwd.clone()),
                            _ => None,
                        }) {
                            Some(password)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // if we could not find a password, create one

                let password = if let Some(password) = password {
                    password
                } else {
                    let password = utils::random_password();
                    gateway.set_section(registry::v1::DeviceSpecCredentials {
                        credentials: vec![registry::v1::Credential::Password(password.clone())],
                    })?;
                    password
                };

                self.registry
                    .update_device(gateway, Default::default())
                    .await
                    .map_err(ReconcileError::temporary)?;

                password
            }
        };

        Ok(password)
    }

    async fn ensure_device(
        &self,
        metadata: &meta::v1::ScopedMetadata,
        app_spec: &TtnAppSpec,
        ttn_app_id: &str,
        spec: &TtnDeviceSpec,
    ) -> Result<(), ReconcileError> {
        let ctx = app_spec.api.to_context()?;
        let ttn_device_id = &metadata.name;

        let ttn_device = self.ttn.get_device(ttn_app_id, ttn_device_id, &ctx).await?;

        log::debug!("TTN device: {:#?}", ttn_device);

        let server = app_spec
            .api
            .region
            .url()
            .map_err(|_| ReconcileError::permanent("Failed to parse TTN API URL"))?;
        let server = server
            .host_str()
            .ok_or_else(|| ReconcileError::permanent("Missing hostname of TTP API"))?;

        let device = ttn::Device {
            ids: ttn::DeviceIds {
                device_id: ttn_device_id.clone(),
                dev_eui: spec.dev_eui.clone(),
                join_eui: spec.app_eui.clone(),
            },
            end_device: ttn::EndDevice {
                name: ttn_device_id.clone(),
                network_server_address: server.into(),
                application_server_address: server.into(),
                join_server_address: server.into(),
            },
            ns_device: ttn::NsDevice {
                multicast: false,
                supports_join: true,
                lorawan_version: spec.lorawan_version.clone(),
                lorawan_phy_version: spec.lorawan_phy_version.clone(),
                mac_settings: convert_args!(hashmap!("supports_32_bit_f_cnt" => true)),
                supports_class_b: spec.supports_class_b,
                supports_class_c: spec.supports_class_b,
                frequency_plan: spec.frequency_plan.clone(),
            },
            js_device: ttn::JsDevice {
                network_server_address: server.into(),
                application_server_address: server.into(),
                join_server_address: server.into(),

                network_server_kek_label: "".into(),
                application_server_kek_label: "".into(),
                application_server_id: "".into(),
                net_id: Default::default(),

                root_keys: ttn::RootKeys {
                    app_key: ttn::Key {
                        key: spec.app_key.clone(),
                    },
                },
            },
        };

        match ttn_device {
            None => {
                log::info!("Creating new device");
                self.ttn
                    .create_device(ttn_app_id, ttn_device_id, device, &ctx)
                    .await?;
            }
            Some(ttn_device) => {
                log::info!("Updating existing device");
                // FIXME: implement
                /*
                if update != ttn_device {
                    self.ttn
                        .update_device(ttn_app_id, ttn_device_id, update, &ctx)
                        .await?;
                }*/
            }
        }

        Ok(())
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
        ttn_app: Value,
        ctx: &ttn::Context,
    ) -> Result<(), ReconcileError> {
        Ok(())
    }
}
