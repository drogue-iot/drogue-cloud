use crate::controller::base::{OperationOutcome, ResourceOperations};
use async_trait::async_trait;
use drogue_client::{core, error::ClientError, openid::TokenProvider, registry, Translator};
use std::ops::Deref;

#[async_trait]
impl<S, TP> ResourceOperations<String, registry::v1::Application, registry::v1::Application> for S
where
    S: Deref<Target = registry::v1::Client<TP>> + Send + Sync,
    TP: TokenProvider,
{
    async fn get(
        &self,
        key: &String,
    ) -> Result<Option<registry::v1::Application>, ClientError<reqwest::Error>> {
        self.get_app(&key, Default::default()).await
    }

    async fn update_if(
        &self,
        original: &registry::v1::Application,
        mut current: registry::v1::Application,
    ) -> Result<OperationOutcome, ()> {
        current
            .update_section(core::v1::Conditions::aggregate_ready)
            .map_err(|_| ())?;

        if original != &current {
            match self.update_app(&current, Default::default()).await {
                Ok(_) => Ok(OperationOutcome::Complete),
                Err(err) => match err {
                    ClientError::Syntax(_) => Ok(OperationOutcome::Complete),
                    _ => Ok(OperationOutcome::RetryNow),
                },
            }
        } else {
            Ok(OperationOutcome::Complete)
        }
    }

    fn ref_output(input: &registry::v1::Application) -> &registry::v1::Application {
        input
    }
}
