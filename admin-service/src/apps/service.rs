use actix_web::ResponseError;
use async_trait::async_trait;
use drogue_cloud_service_api::admin::{Members, TransferOwnership};
use drogue_cloud_service_api::auth::user::UserInformation;

#[async_trait]
pub trait AdminService: Clone {
    type Error: ResponseError;

    async fn transfer(
        &self,
        identity: &UserInformation,
        app_id: String,
        transfer: TransferOwnership,
    ) -> Result<(), Self::Error>;

    async fn cancel(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error>;
    async fn accept(&self, identity: &UserInformation, app_id: String) -> Result<(), Self::Error>;

    async fn get_members(
        &self,
        identity: &UserInformation,
        app_id: String,
    ) -> Result<Members, Self::Error>;
    async fn set_members(
        &self,
        identity: &UserInformation,
        app_id: String,
        members: Members,
    ) -> Result<(), Self::Error>;
}
