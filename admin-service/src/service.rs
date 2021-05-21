use actix_http::ResponseError;
use drogue_cloud_service_api::admin::TransferOwnership;
use drogue_cloud_service_api::auth::user::UserInformation;

#[async_trait]
pub trait AdminService: Clone {
    type Error: ResponseError;

    async fn transfer(
        &self,
        identity: &UserInformation,
        transfer: TransferOwnership,
    ) -> Result<(), Self::Error>;

    async fn cancel(&self, identity: &UserInformation) -> Result<(), Self::Error>;
    async fn accept(&self, identity: &UserInformation) -> Result<(), Self::Error>;
}
