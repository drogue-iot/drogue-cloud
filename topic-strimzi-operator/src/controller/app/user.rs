use super::{condition_ready, retry, ConstructContext, ANNOTATION_APP_NAME, LABEL_KAFKA_CLUSTER};
use crate::controller::ControllerConfig;
use async_trait::async_trait;
use drogue_client::registry::v1::DownstreamSpec;
use drogue_client::{
    registry::v1::{Application, KafkaAppStatus, KafkaUserStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{self, OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use k8s_openapi::{api::core::v1::Secret, ByteString};
use kube::{
    api::{ApiResource, DynamicObject},
    Api, Resource,
};
use operator_framework::{
    install::Delete,
    process::{create_or_update, create_or_update_by},
    utils::UseOrCreate,
};
use serde_json::json;

const KEY_PASSWORD: &str = "password";

pub struct CreateUser<'o> {
    pub users_api: &'o Api<DynamicObject>,
    pub users_resource: &'o ApiResource,
    pub secrets_api: &'o Api<Secret>,
    pub config: &'o ControllerConfig,
}

impl CreateUser<'_> {
    async fn ensure_kafka_user(
        &self,
        app: String,
        // a user provided password to apply
        password: Option<String>,
    ) -> Result<(DynamicObject, String), ReconcileError> {
        let user_name = make_kafka_resource_name(ResourceType::Users(&app));
        let topic_name = make_kafka_resource_name(ResourceType::Events(&app));
        let password_name = make_kafka_resource_name(ResourceType::Passwords(&app));

        let user = create_or_update_by(
            self.users_api,
            Some(self.config.topic_namespace.clone()),
            &user_name,
            |meta| {
                let mut user = DynamicObject::new(&topic_name, self.users_resource)
                    .within(&self.config.topic_namespace);
                *user.meta_mut() = meta;
                user
            },
            |this, that| this.metadata == that.metadata && this.data == that.data,
            |mut user| {
                user.metadata.labels.use_or_create(|labels| {
                    // set target cluster
                    labels.insert(LABEL_KAFKA_CLUSTER.into(), self.config.cluster_name.clone());
                });
                user.metadata.annotations.use_or_create(|annotations| {
                    annotations.insert(ANNOTATION_APP_NAME.into(), app.clone());
                });

                let password = match password.is_some() {
                    true => Some(json!({
                        "valueFrom": {
                            "secretKeyRef": {
                                "name": password_name,
                                "key": KEY_PASSWORD,
                            }
                        }
                    })),
                    false => None,
                };

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

                if let Some(password) = password {
                    user.data["spec"]["authentication"]["password"] = password;
                }

                Ok::<_, ReconcileError>(user)
            },
        )
        .await?
        .resource();

        // reconcile password

        if let Some(password) = password {
            // create/update secret
            create_or_update(
                self.secrets_api,
                Some(&self.config.topic_namespace),
                password_name,
                |mut secret| {
                    secret.metadata.annotations.use_or_create(|annotations| {
                        annotations.insert(ANNOTATION_APP_NAME.into(), app.clone());
                    });

                    secret.data.use_or_create(|data| {
                        data.clear();
                        data.insert(KEY_PASSWORD.into(), ByteString(password.into_bytes()));
                    });

                    Ok::<_, ReconcileError>(secret)
                },
            )
            .await?;
        } else {
            // delete secret
            self.secrets_api
                .delete_optionally(&password_name, &Default::default())
                .await?;
        }

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
        let password = find_user_password(&ctx.app);
        let (user, user_name) = self
            .ensure_kafka_user(ctx.app.metadata.name.clone(), password)
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
            Some(user) => self.secrets.get(user).await.ok(),
        };

        // construct the user status

        let user = match (
            user_ready,
            ctx.app_user_name.as_ref().cloned(),
            app_user_secret.and_then(|s| {
                s.data
                    .as_ref()
                    .and_then(|data| data.get(KEY_PASSWORD))
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

/// Find a configured Kafka user password from an application.
fn find_user_password(app: &Application) -> Option<String> {
    app.section::<DownstreamSpec>()
        .and_then(|s| s.ok())
        .and_then(|spec| spec.password)
}
