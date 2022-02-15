use crate::service::{error::PostgresManagementServiceError, PostgresManagementService};
use async_trait::async_trait;
use drogue_cloud_admin_service::apps::AdminService;
use drogue_cloud_database_common::{
    auth::ensure_with,
    error::ServiceError,
    models::{
        app::{self, ApplicationAccessor, PostgresApplicationAccessor},
        Lock,
    },
};
use drogue_cloud_registry_events::EventSender;
use drogue_cloud_service_api::admin::{MemberEntry, Members, TransferOwnership};
use drogue_cloud_service_api::auth::user::{authz::Permission, UserInformation};
use drogue_cloud_service_common::keycloak::KeycloakClient;
use indexmap::map::IndexMap;
use tracing::instrument;

#[async_trait]
impl<S, K> AdminService for PostgresManagementService<S, K>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    type Error = PostgresManagementServiceError<S::Error>;

    #[instrument(skip(self))]
    async fn transfer(
        &self,
        identity: &UserInformation,
        app_id: String,
        transfer: TransferOwnership,
    ) -> Result<(), Self::Error> {
        // pre-flight check

        if transfer.new_user.is_empty() {
            return Err(ServiceError::BadRequest("Invalid user id".into()).into());
        }

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        // ensure we are permitted to do the change

        ensure_with(&app, identity, Permission::Owner, || ServiceError::NotFound)?;

        // retrieve the new user ID from keycloak
        let new_user = match self
            .keycloak
            .id_from_username(transfer.new_user.as_str())
            .await
        {
            Ok(u) => u,
            // If the username does not exist in keycloak it's an error !
            Err(_) => {
                return Err(ServiceError::BadRequest(format!(
                    "Username {} does not exist",
                    transfer.new_user
                ))
                .into());
            }
        };

        log::debug!(
            "Initiate transfer - new transfer owner: {}, identity: {:?}",
            &new_user,
            identity.user_id()
        );

        // make the change

        accessor
            .update_transfer(app.name, identity.user_id().map(Into::into), Some(new_user))
            .await?;

        // commit

        t.commit().await?;

        // done

        Ok(())
    }

    #[instrument(skip(self))]
    async fn cancel(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        // ensure we are permitted to do the change

        // the app receiver is allowed to decline the transfer
        if app.transfer_owner.as_deref() == identity.user_id()
            // The app owner (who initiated the transfer) cancels the transfer
            || app.owner.as_deref() == identity.user_id()
        {
            // make the change

            accessor
                .update_transfer(app.name, app.owner.map(Into::into), None)
                .await?;

            // commit

            t.commit().await?;

            // done

            Ok(())
        } else {
            Err(ServiceError::NotFound.into())
        }
    }

    #[instrument(skip(self))]
    async fn accept(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        log::debug!(
            "Transfer - transfer owner: {:?}, identity: {:?}",
            app.transfer_owner,
            identity.user_id()
        );

        // make the change

        if app.transfer_owner.as_deref() == identity.user_id() {
            accessor
                .update_transfer(app.name, identity.user_id().map(Into::into), None)
                .await?;

            // commit

            t.commit().await?;

            Ok(())
        } else {
            Err(ServiceError::NotFound.into())
        }
    }

    #[instrument(skip(self))]
    async fn read_transfer_state(
        &self,
        identity: &UserInformation,
        app_id: String,
    ) -> Result<Option<TransferOwnership>, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        // the app receiver is allowed to read the transfer state
        if app.transfer_owner.as_deref() == identity.user_id()
            // The app owner as well
            || app.owner.as_deref() == identity.user_id()
        {
            match app.transfer_owner {
                Some(new_user) => {
                    // retrieve the user name
                    match self.keycloak.username_from_id(new_user.as_str()).await {
                        Ok(u) => Ok(Some(TransferOwnership { new_user: u })),
                        // If the id does not exist in keycloak
                        Err(_) => Err(ServiceError::Internal(
                            "Transfer owner is no longer a keycloak user".to_string(),
                        )
                        .into()),
                    }
                }
                None => Ok(None),
            }
        } else {
            Err(ServiceError::NotFound.into())
        }
    }

    #[instrument(skip(self))]
    async fn get_members(
        &self,
        identity: &UserInformation,
        app_id: String,
    ) -> Result<Members, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::None).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        // ensure we are permitted to perform the operation

        ensure_with(&app, identity, Permission::Admin, || ServiceError::NotFound)?;

        // get operation
        let mut members: IndexMap<String, MemberEntry> = IndexMap::new();
        for (k, v) in &app.members {
            // empty values are allowed. (e.g. to share an app with the whole word)
            if k.is_empty() {
                members.insert(k.clone(), MemberEntry { role: v.role });
            } else {
                match self.keycloak.username_from_id(k).await {
                    Ok(u) => members.insert(u, MemberEntry { role: v.role }),
                    // If the id does not exist in keycloak we skip it
                    Err(_) => None,
                };
            }
        }

        Ok(Members {
            resource_version: Some(app.resource_version.to_string()),
            members,
        })
    }

    #[instrument(skip(self))]
    async fn set_members(
        &self,
        identity: &UserInformation,
        app_id: String,
        members: Members,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or(ServiceError::NotFound)?;

        if let Some(expected_version) = &members.resource_version {
            if expected_version != &app.resource_version.to_string() {
                return Err(ServiceError::OptimisticLockFailed.into());
            }
        }

        // ensure we are permitted to perform the operation

        ensure_with(&app, identity, Permission::Admin, || ServiceError::NotFound)?;

        // get users id from usernames

        let mut id_members: IndexMap<String, app::MemberEntry> = IndexMap::new();
        for (k, v) in &members.members {
            if !k.is_empty() {
                match self.keycloak.id_from_username(k.as_str()).await {
                    Ok(u) => {
                        id_members.insert(u, app::MemberEntry { role: v.role });
                    }
                    // If the username does not exist in keycloak it's an error !
                    Err(_) => {
                        return Err(ServiceError::BadRequest(format!(
                            "Username {} does not exist",
                            k
                        ))
                        .into());
                    }
                };
                // empty values are allowed. (e.g. to share an app with the whole word)
            } else {
                id_members.insert(k.clone(), app::MemberEntry { role: v.role });
            }
        }

        // set operation

        accessor
            .set_members(&app_id, id_members)
            .await
            .map(|_| ())?;

        // commit

        t.commit().await?;

        Ok(())
    }
}
