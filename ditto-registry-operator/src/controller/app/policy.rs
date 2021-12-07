use super::{ConstructContext, DeconstructContext};
use crate::{
    controller::{policy_id, ControllerConfig},
    ditto::{
        self,
        api::PolicyOperation,
        data::{Permission, Permissions, Policy, PolicyEntry, Resource, Subject},
    },
};
use async_trait::async_trait;
use drogue_client::openid::OpenIdTokenProvider;
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use http::StatusCode;
use indexmap::IndexMap;

pub struct CreatePolicy<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o OpenIdTokenProvider,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreatePolicy<'o> {
    fn type_name(&self) -> String {
        "CreatePolicy".into()
    }

    async fn run(
        &self,
        ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let policy_id = policy_id(&ctx.app);

        let resp = self
            .ditto
            .request(
                self.provider,
                PolicyOperation::CreateOrUpdate(Policy {
                    policy_id,

                    entries: {
                        let mut map = IndexMap::new();

                        // Grant the admin (us) access to all policies and things.
                        map.insert(
                            "ADMIN".to_string(),
                            PolicyEntry {
                                subjects: {
                                    let mut subjects = IndexMap::new();
                                    subjects.insert(
                                        "keycloak:drogue-admin".to_string(),
                                        Subject {
                                            r#type: "Admin access".to_string(),
                                        },
                                    );
                                    subjects
                                },
                                resources: {
                                    let mut map = IndexMap::new();
                                    map.insert(
                                        Resource::policy("/"),
                                        Permissions::grant([Permission::Read, Permission::Write]),
                                    );
                                    map.insert(
                                        Resource::thing("/"),
                                        Permissions::grant([Permission::Read, Permission::Write]),
                                    );
                                    map
                                },
                            },
                        );

                        // Grant the connection access to all things
                        map.insert(
                            "CONNECTION".to_string(),
                            PolicyEntry {
                                subjects: {
                                    let mut subjects = IndexMap::new();
                                    subjects.insert(
                                        "pre-authenticated:drogue-cloud".to_string(),
                                        Subject {
                                            r#type: "Connection to Drogue IoT Kafka topic"
                                                .to_string(),
                                        },
                                    );
                                    subjects
                                },
                                resources: {
                                    let mut map = IndexMap::new();
                                    map.insert(
                                        Resource::thing("/"),
                                        Permissions::grant([Permission::Read, Permission::Write]),
                                    );
                                    map
                                },
                            },
                        );

                        // FIXME: currently grant users access to all things
                        map.insert(
                            "USERS".to_string(),
                            PolicyEntry {
                                subjects: {
                                    let mut subjects = IndexMap::new();
                                    subjects.insert(
                                        "keycloak:drogue-user".to_string(),
                                        Subject {
                                            r#type: "All users".to_string(),
                                        },
                                    );
                                    subjects
                                },
                                resources: {
                                    let mut map = IndexMap::new();
                                    map.insert(
                                        Resource::thing("/"),
                                        Permissions::grant([Permission::Read, Permission::Write]),
                                    );
                                    map
                                },
                            },
                        );

                        map
                    },
                }),
            )
            .await
            .map_err(map_ditto_error)?;

        log::debug!("Response: {:#?}", resp);

        // Done
        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct DeletePolicy<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o OpenIdTokenProvider,
}

impl<'o> DeletePolicy<'o> {
    pub async fn run(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        let resp = self
            .ditto
            .request(self.provider, PolicyOperation::Delete(policy_id(&ctx.app)))
            .await;

        if let Err(ditto::Error::Response(StatusCode::NOT_FOUND)) = resp {
            // already gone
            log::debug!("Policy was already gone");
            return Ok(());
        }

        let resp = resp.map_err(map_ditto_error)?;
        log::debug!("Response: {:#?}", resp);

        Ok(())
    }
}

fn map_ditto_error(err: ditto::Error) -> ReconcileError {
    match err.is_temporary() {
        true => ReconcileError::temporary(err),
        false => ReconcileError::permanent(err),
    }
}
