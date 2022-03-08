mod account;

use account::*;

use crate::{controller::ControllerConfig, InstanceConfiguration, MgmtConfiguration};
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::CommonMetadataMut,
    openid::{OpenIdTokenProvider, TokenProvider},
    registry::{self, v1::KafkaAppStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, CONDITION_RECONCILED},
    reconciler::{
        operation::HasFinalizer,
        progress::{Progressor, ResourceAccessor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use std::ops::Deref;

const FINALIZER: &str = "rhoas-user";

pub struct ApplicationController<TP: TokenProvider> {
    config: ControllerConfig,
    registry: registry::v1::Client<TP>,
    api_token_provider: Option<OpenIdTokenProvider>,
}

impl<TP: TokenProvider> ApplicationController<TP> {
    pub async fn new(
        config: ControllerConfig,
        registry: registry::v1::Client<TP>,
    ) -> anyhow::Result<Self> {
        let api_token_provider = match &config.api.oauth2 {
            Some(token) => Some(token.clone().discover_from().await?),
            None => None,
        };

        Ok(Self {
            config,
            registry,
            api_token_provider,
        })
    }

    async fn make_configuration(
        &self,
    ) -> Result<(MgmtConfiguration, InstanceConfiguration), ReconcileError> {
        let user_agent = Some(format!(
            "Drogue IoT Cloud/{}",
            drogue_cloud_service_api::version::VERSION
        ));
        let client = reqwest::Client::new();

        let token = match &self.api_token_provider {
            Some(token_provider) => Some(
                token_provider
                    .provide_token()
                    .await
                    .map_err(|err| {
                        ReconcileError::temporary(format!(
                            "Failed to fetch access token for API: {err}"
                        ))
                    })?
                    .access_token,
            ),

            None => None,
        };

        // the management API uses "bearer token"
        let mgmt_config = MgmtConfiguration {
            base_path: self.config.api.mgmt_base_path(),
            user_agent: user_agent.clone(),
            client: client.clone(),
            basic_auth: None,
            oauth_access_token: None,
            bearer_access_token: token.clone(),
            api_key: None,
        };

        // the management API uses "oauth token"
        let instance_config = InstanceConfiguration {
            base_path: self.config.api.instance_base_path(),
            user_agent,
            client,
            basic_auth: None,
            oauth_access_token: token,
            bearer_access_token: None,
            api_key: None,
        };

        Ok((mgmt_config, instance_config))
    }
}

#[async_trait]
impl<TP: TokenProvider>
    ControllerOperation<String, registry::v1::Application, registry::v1::Application>
    for ApplicationController<TP>
{
    async fn process_resource(
        &self,
        application: registry::v1::Application,
    ) -> Result<ProcessOutcome<registry::v1::Application>, ReconcileError> {
        let (mgmt_config, instance_config) = self.make_configuration().await?;

        ReconcileProcessor(ApplicationReconciler {
            config: &self.config,
            registry: &self.registry,
            mgmt_config: &mgmt_config,
            instance_config: &instance_config,
        })
        .reconcile(application)
        .await
    }

    async fn recover(
        &self,
        message: &str,
        mut app: registry::v1::Application,
    ) -> Result<registry::v1::Application, ()> {
        let mut conditions = app
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok().map(|s| s.conditions))
            .unwrap_or_default();

        conditions.update(CONDITION_RECONCILED, ReadyState::Failed(message.into()));

        app.finish_ready::<KafkaAppStatus>(conditions, app.metadata.generation)
            .map_err(|_| ())?;

        Ok(app)
    }
}

impl<TP: TokenProvider> Deref for ApplicationController<TP> {
    type Target = registry::v1::Client<TP>;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

pub struct ConstructContext {
    pub app: registry::v1::Application,
    pub events_topic_name: Option<String>,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<KafkaAppStatus>,
}

pub struct ApplicationReconciler<'a, TP: TokenProvider> {
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client<TP>,
    pub mgmt_config: &'a MgmtConfiguration,
    pub instance_config: &'a InstanceConfiguration,
}

#[async_trait]
impl<'a, TP: TokenProvider> Reconciler for ApplicationReconciler<'a, TP> {
    type Input = registry::v1::Application;
    type Output = registry::v1::Application;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    async fn eval_state(
        &self,
        app: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        Self::eval_by_finalizer(
            true,
            app,
            FINALIZER,
            |app| ConstructContext {
                app,
                events_topic_name: None,
            },
            |app| {
                let status = app.section::<KafkaAppStatus>().and_then(|s| s.ok());
                DeconstructContext { app, status }
            },
            |app| app,
        )
    }

    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(HasFinalizer(FINALIZER)),
            Box::new(CreateServiceAccount {
                config: self.config,
                mgmt_config: self.mgmt_config,
                instance_config: self.instance_config,
            }),
        ])
        .run_with::<KafkaAppStatus>(ctx)
        .await
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // delete

        DeleteServiceAccount {
            config: self.config,
            mgmt_config: self.mgmt_config,
            instance_config: self.instance_config,
        }
        .run(&mut ctx)
        .await?;

        // remove finalizer

        ctx.app.metadata.remove_finalizer(FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }
}

impl ResourceAccessor for ConstructContext {
    type Resource = registry::v1::Application;

    fn resource(&self) -> &registry::v1::Application {
        &self.app
    }

    fn resource_mut(&mut self) -> &mut registry::v1::Application {
        &mut self.app
    }

    fn into(self) -> registry::v1::Application {
        self.app
    }

    fn conditions(&self) -> Conditions {
        self.app
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}
