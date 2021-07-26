use crate::controller::base::{OperationOutcome, ResourceOperations};
use async_trait::async_trait;
use drogue_client::{error::ClientError, registry};
use std::ops::Deref;

#[async_trait]
impl<S> ResourceOperations<String, registry::v1::Application, registry::v1::Application> for S
where
    S: Deref<Target = registry::v1::Client> + Send + Sync,
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
        current: &registry::v1::Application,
    ) -> Result<OperationOutcome, ()> {
        if original != current {
            match self.update_app(current, Default::default()).await {
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
