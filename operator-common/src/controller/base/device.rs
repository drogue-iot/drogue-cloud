use crate::controller::base::{OperationOutcome, ResourceOperations};
use async_trait::async_trait;
use drogue_client::{core, error::ClientError, registry, Translator};
use futures::try_join;
use std::ops::Deref;

#[async_trait]
impl<S>
    ResourceOperations<
        (String, String),
        (registry::v1::Application, registry::v1::Device),
        registry::v1::Device,
    > for S
where
    S: Deref<Target = registry::v1::Client> + Send + Sync,
{
    async fn get(
        &self,
        key: &(String, String),
    ) -> Result<
        Option<(registry::v1::Application, registry::v1::Device)>,
        ClientError<reqwest::Error>,
    > {
        Ok(
            match try_join!(
                self.get_app(&key.0, Default::default()),
                self.get_device(&key.0, &key.1, Default::default()),
            )? {
                (Some(app), Some(device)) => Some((app, device)),
                _ => None,
            },
        )
    }

    async fn update_if(
        &self,
        original: &registry::v1::Device,
        mut current: registry::v1::Device,
    ) -> Result<OperationOutcome, ()> {
        current
            .update_section(core::v1::Conditions::aggregate_ready)
            .map_err(|_| ())?;

        if original != &current {
            match self.update_device(&current, Default::default()).await {
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

    fn ref_output(
        input: &(registry::v1::Application, registry::v1::Device),
    ) -> &registry::v1::Device {
        &input.1
    }
}
