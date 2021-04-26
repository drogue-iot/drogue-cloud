use crate::{
    data::{ApiKey, ApiKeyCreated, ApiKeyCreationOptions},
    service::ApiKeyService,
};
use actix_web::ResponseError;
use async_trait::async_trait;
use drogue_cloud_service_common::auth::Identity;
use std::fmt::Formatter;

#[derive(Clone)]
pub struct MockApiKeyService;

#[derive(Debug)]
pub struct MockError;

impl core::fmt::Display for MockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "MockError")
    }
}

impl ResponseError for MockError {}

#[async_trait]
impl ApiKeyService for MockApiKeyService {
    type Error = MockError;

    async fn create(
        &self,
        _: &dyn Identity,
        _: ApiKeyCreationOptions,
    ) -> Result<ApiKeyCreated, Self::Error> {
        todo!()
    }

    async fn delete(&self, _: &dyn Identity, _: String) -> Result<(), Self::Error> {
        todo!()
    }

    async fn list(&self, _: &dyn Identity) -> Result<Vec<ApiKey>, Self::Error> {
        todo!()
    }

    async fn authenticate(&self, _: &str, _: &str) -> Result<bool, Self::Error> {
        todo!()
    }
}
