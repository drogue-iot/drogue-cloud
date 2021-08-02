use super::{condition_ready, retry, ApplicationReconciler, ConstructContext};
use crate::{controller::ControllerConfig, data::*};
use async_trait::async_trait;
use drogue_client::Translator;
use drogue_cloud_operator_common::controller::reconciler::progress::{
    self, OperationOutcome, ProgressOperation,
};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{ApiResource, DynamicObject},
    Api,
};

pub struct CreateUser<'o> {
    pub api: &'o Api<DynamicObject>,
    pub resource: &'o ApiResource,
    pub config: &'o ControllerConfig,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateUser<'o> {
    fn type_name(&self) -> String {
        "CreateUser".into()
    }

    async fn run(
        &self,
        mut ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let (user, user_name) = ApplicationReconciler::ensure_kafka_user(
            &self.api,
            &self.resource,
            &self.config,
            ctx.app.metadata.name.clone(),
        )
        .await?;

        ctx.app_user = Some(user);
        ctx.app_user_name = Some(user_name);

        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct UserReady<'o> {
    pub config: &'o ControllerConfig,
    pub secrets: &'o Api<Secret>,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for UserReady<'o> {
    fn type_name(&self) -> String {
        "UserReady".into()
    }

    async fn run(&self, mut ctx: ConstructContext) -> progress::Result<ConstructContext> {
        let user_ready = ctx
            .app_user
            .as_ref()
            .and_then(|user| condition_ready("Ready", user))
            .unwrap_or_default();

        // load the user secret

        let app_user_secret = match ctx.app_user_name.as_ref() {
            None => None,
            Some(user) => self.secrets.get(&user).await.ok(),
        };

        // construct the user status

        let user = match (
            user_ready,
            ctx.app_user_name.as_ref().cloned(),
            app_user_secret.and_then(|s| {
                s.data
                    .get("password")
                    .and_then(|s| String::from_utf8(s.0.clone()).ok())
            }),
        ) {
            (true, Some(username), Some(password)) => Some(KafkaUserStatus {
                username,
                password,
                mechanism: "SCRAM-SHA-512".into(),
            }),
            _ => None,
        };

        let user_status = user.is_some();

        // update the user section

        ctx.app.update_section(|mut status: KafkaAppStatus| {
            status.user = user;
            status
        })?;

        // done

        match user_ready && user_status {
            true => Ok(OperationOutcome::Continue(ctx)),
            false => retry(ctx),
        }
    }
}
