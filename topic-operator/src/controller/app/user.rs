use super::{condition_ready, retry, ConstructContext, ANNOTATION_APP_NAME, LABEL_KAFKA_CLUSTER};
use crate::controller::ControllerConfig;
use async_trait::async_trait;
use drogue_client::{
    registry::v1::{KafkaAppStatus, KafkaUserStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{self, OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{ApiResource, DynamicObject},
    Api, Resource,
};
use operator_framework::process::create_or_update_by;
use serde_json::json;

pub struct CreateUser<'o> {
    pub api: &'o Api<DynamicObject>,
    pub resource: &'o ApiResource,
    pub config: &'o ControllerConfig,
}

impl CreateUser<'_> {
    async fn ensure_kafka_user(
        kafka_users: &Api<DynamicObject>,
        kafka_user_resource: &ApiResource,
        config: &ControllerConfig,
        app: String,
    ) -> Result<(DynamicObject, String), ReconcileError> {
        let user_name = make_kafka_resource_name(ResourceType::Users(app.clone()));
        let topic_name = make_kafka_resource_name(ResourceType::Events(app.clone()));

        let user = create_or_update_by(
            &kafka_users,
            Some(config.topic_namespace.clone()),
            &user_name,
            |meta| {
                let mut user = DynamicObject::new(&topic_name, &kafka_user_resource)
                    .within(&config.topic_namespace);
                *user.meta_mut() = meta;
                user
            },
            |this, that| this.metadata == that.metadata && this.data == that.data,
            |mut user| {
                // set target cluster
                user.metadata
                    .labels
                    .insert(LABEL_KAFKA_CLUSTER.into(), config.cluster_name.clone());
                user.metadata
                    .annotations
                    .insert(ANNOTATION_APP_NAME.into(), app.clone());
                // set config
                user.data["spec"] = json!({
                    "authentication": {
                        "type": "scram-sha-512",
                    },
                    "authorization": {
                        "acls": [
                            {
                                "host": "*",
                                "operation": "Read",
                                "resource": {
                                    "type": "topic",
                                    "name": topic_name,
                                    "patternType": "literal",
                                },
                            },
                            {
                                "host": "*",
                                "operation": "Read",
                                "resource": {
                                    "type": "group",
                                    "name": "*",
                                    "patternType": "literal",
                                }
                            }
                        ],
                        "type": "simple",
                    },
                    "template": {
                        "secret": {
                            "metadata": {
                                "annotations": {
                                   ANNOTATION_APP_NAME: app,
                                }
                            },
                        }
                    }
                });

                Ok::<_, ReconcileError>(user)
            },
        )
        .await?
        .resource();

        // done

        Ok((user, user_name))
    }
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
        let (user, user_name) = Self::ensure_kafka_user(
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
