use super::ConstructContext;
use crate::{
    controller::{app::DeconstructContext, ControllerConfig},
    InstanceConfiguration, MgmtConfiguration,
};
use async_trait::async_trait;
use drogue_client::{
    registry::v1::{KafkaAppStatus, KafkaUserStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::reconciler::progress::{
    OperationOutcome, ProgressOperation,
};
use drogue_cloud_operator_common::controller::reconciler::ReconcileError;
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use log::Level;
use rhoas_kafka_instance_sdk::{
    apis::{
        acls_api::{create_acl, delete_acls, CreateAclError},
        Error as InstanceError, ResponseContent,
    },
    models::{AclBinding, AclOperation, AclPatternType, AclPermissionType, AclResourceType},
};
use rhoas_kafka_management_sdk::{
    apis::security_api::{create_service_account, delete_service_account_by_id},
    models::{ServiceAccountListItem, ServiceAccountRequest},
};

pub struct CreateServiceAccount<'o> {
    pub config: &'o ControllerConfig,
    pub mgmt_config: &'o MgmtConfiguration,
    pub instance_config: &'o InstanceConfiguration,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateServiceAccount<'o> {
    fn type_name(&self) -> String {
        "CreateServiceAccount".into()
    }

    async fn run(
        &self,
        mut ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let user_name = make_kafka_resource_name(ResourceType::Users(&ctx.app.metadata.name));
        let topic_name = make_kafka_resource_name(ResourceType::Events(&ctx.app.metadata.name));

        // check if we have an account already
        let accounts = find_service_accounts(self.mgmt_config, &user_name).await?;
        // check the last known status
        let status = ctx
            .app
            .section()
            .transpose()?
            .and_then(|status: KafkaAppStatus| status.user)
            .unwrap_or_default();

        // if we have an account, and it matches the record username, ...
        if accounts.len() == 1 && accounts[0].client_id == Some(status.username) {
            // ... we are done!
            return Ok(OperationOutcome::Continue(ctx));
        }

        // otherwise ...

        // delete the service account and ACLs
        delete_service_account(self.mgmt_config, self.instance_config, &user_name).await?;

        // create new account

        let sa = create_service_account(
            self.mgmt_config,
            ServiceAccountRequest {
                name: user_name,
                description: Some(format!(
                    "Default Kafka topic user for '{}'",
                    ctx.app.metadata.name
                )),
            },
        )
        .await
        .map_err(|err| {
            ReconcileError::temporary(format!("Failed to create service account: {err}"))
        })?;

        // extract credentials

        let (username, password) = match (sa.client_id, sa.client_secret) {
            (Some(username), Some(password)) => (username, password),
            _ => {
                return Err(ReconcileError::permanent(format!(
                    "Missing credentials when creating a service account."
                )));
            }
        };

        // create ACLs

        self.create_acl(AclBinding {
            resource_type: AclResourceType::GROUP,
            resource_name: "*".to_string(),
            pattern_type: AclPatternType::LITERAL,
            principal: username.clone(),
            operation: AclOperation::ALL,
            permission: AclPermissionType::ALLOW,
        })
        .await?;
        self.create_acl(AclBinding {
            resource_type: AclResourceType::TOPIC,
            resource_name: topic_name,
            pattern_type: AclPatternType::LITERAL,
            principal: username.clone(),
            operation: AclOperation::ALL,
            permission: AclPermissionType::ALLOW,
        })
        .await?;

        // update the user section

        ctx.app.update_section(|mut status: KafkaAppStatus| {
            status.user = Some(KafkaUserStatus {
                username,
                password,
                mechanism: self.config.sasl_mechanism.clone(),
            });
            status
        })?;

        // done, but re-try immediately, which will persist before we are doing anything else

        Ok(OperationOutcome::Retry(ctx, None))
    }
}

impl<'o> CreateServiceAccount<'o> {
    async fn create_acl(&self, binding: AclBinding) -> Result<(), ReconcileError> {
        match create_acl(self.instance_config, binding).await {
            // yay
            Ok(_) => {
                log::debug!("Successfully created ACL entry");
                Ok(())
            }
            // failed request, no reason to retry
            Err(InstanceError::ResponseError(ResponseContent {
                entity: Some(CreateAclError::Status400(reason)),
                ..
            })) => {
                Err(ReconcileError::permanent(format!("Bad request: {reason:?}")).log(Level::Info))
            }
            // any error, retry
            Err(err) => {
                Err(ReconcileError::temporary(format!("Failed request: {err}")).log(Level::Info))
            }
        }
    }
}

pub struct DeleteServiceAccount<'o> {
    pub config: &'o ControllerConfig,
    pub mgmt_config: &'o MgmtConfiguration,
    pub instance_config: &'o InstanceConfiguration,
}

impl<'o> DeleteServiceAccount<'o> {
    pub async fn run(&self, ctx: &mut DeconstructContext) -> Result<(), ReconcileError> {
        let user_name = make_kafka_resource_name(ResourceType::Users(&ctx.app.metadata.name));
        //let topic_name = make_kafka_resource_name(ResourceType::Events(&ctx.app.metadata.name));

        // delete service account

        delete_service_account(self.mgmt_config, self.instance_config, &user_name).await?;

        // done

        Ok(())
    }
}

/// Delete service account and ACLs by name.
async fn delete_service_account(
    mgmt_config: &MgmtConfiguration,
    instance_config: &InstanceConfiguration,
    name: &str,
) -> Result<(), ReconcileError> {
    // ACLs first, as we use the account to identify the state

    delete_acls(instance_config, None, None, None, Some(name), None, None)
        .await
        .map_err(|err| ReconcileError::temporary(format!("Failed to delete ACLs: {err}")))?;

    // then the accounts

    for sa in find_service_accounts(mgmt_config, name).await? {
        if let Some(id) = sa.id {
            delete_service_account_by_id(mgmt_config, &id)
                .await
                .map_err(|err| {
                    ReconcileError::temporary(format!(
                        "Failed to delete service account ({id}): {err}"
                    ))
                })?;
        }
    }

    Ok(())
}

async fn find_service_accounts(
    config: &MgmtConfiguration,
    name: &str,
) -> Result<Vec<ServiceAccountListItem>, ReconcileError> {
    Ok(
        rhoas_kafka_management_sdk::apis::security_api::get_service_accounts(config)
            .await
            .map_err(|err| {
                ReconcileError::temporary(format!("Failed to query service accounts: {err}"))
            })?
            .items
            .into_iter()
            .filter(|sa| sa.name.as_deref() == Some(name))
            .collect(),
    )
}
