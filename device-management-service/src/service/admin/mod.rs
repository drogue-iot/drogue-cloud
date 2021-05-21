use crate::service::{error::PostgresManagementServiceError, PostgresManagementService};
use async_trait::async_trait;
use drogue_cloud_admin_service::apps::AdminService;
use drogue_cloud_database_common::{
    auth::ensure_with,
    error::ServiceError,
    models::{
        app::{ApplicationAccessor, PostgresApplicationAccessor},
        Lock,
    },
};
use drogue_cloud_registry_events::EventSender;
use drogue_cloud_service_api::{admin::TransferOwnership, auth::user::UserInformation};

#[async_trait]
impl<S> AdminService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    type Error = PostgresManagementServiceError<S::Error>;

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
        let app = app.ok_or_else(|| ServiceError::NotFound)?;

        // ensure we are permitted to do the change

        ensure_with(&app, identity, || ServiceError::NotFound)?;

        // make the change

        accessor
            .update_transfer(
                app.name,
                identity.user_id().map(Into::into),
                Some(transfer.new_user),
            )
            .await?;

        // commit

        t.commit().await?;

        // done

        Ok(())
    }

    async fn cancel(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or_else(|| ServiceError::NotFound)?;

        // ensure we are permitted to do the change

        ensure_with(&app, identity, || ServiceError::NotFound)?;

        // make the change

        accessor
            .update_transfer(app.name, identity.user_id().map(Into::into), None)
            .await?;

        // commit

        t.commit().await?;

        // done

        Ok(())
    }

    async fn accept(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // retrieve app

        let app = accessor.get(&app_id, Lock::ForUpdate).await?;
        let app = app.ok_or_else(|| ServiceError::NotFound)?;

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
}
