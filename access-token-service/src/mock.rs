use crate::service::AccessTokenService;
use async_trait::async_trait;
use drogue_cloud_service_api::webapp::ResponseError;
use drogue_cloud_service_api::{
    auth::user::{UserDetails, UserInformation},
    token::{AccessToken, AccessTokenCreated, AccessTokenCreationOptions},
};
use std::fmt::Formatter;

#[derive(Clone)]
pub struct MockAccessTokenService;

#[derive(Debug)]
pub struct MockError;

impl core::fmt::Display for MockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "MockError")
    }
}

impl ResponseError for MockError {}

#[async_trait]
impl AccessTokenService for MockAccessTokenService {
    type Error = MockError;

    async fn create(
        &self,
        _: &UserInformation,
        _: AccessTokenCreationOptions,
    ) -> Result<AccessTokenCreated, Self::Error> {
        todo!()
    }

    async fn delete(&self, _: &UserInformation, _: String) -> Result<(), Self::Error> {
        todo!()
    }

    async fn list(&self, _: &UserInformation) -> Result<Vec<AccessToken>, Self::Error> {
        todo!()
    }

    async fn authenticate(&self, _: &str, _: &str) -> Result<Option<UserDetails>, Self::Error> {
        todo!()
    }
}
