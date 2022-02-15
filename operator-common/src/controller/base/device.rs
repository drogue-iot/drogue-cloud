use crate::controller::{base::ResourceOperations, reconciler::ReconcileError};
use async_trait::async_trait;
use drogue_client::{core, error::ClientError, openid::TokenProvider, registry, Translator};
use futures::try_join;
use std::ops::Deref;

#[async_trait]
impl<S, TP>
    ResourceOperations<
        (String, String),
        (registry::v1::Application, registry::v1::Device),
        registry::v1::Device,
    > for S
where
    S: Deref<Target = registry::v1::Client<TP>> + Send + Sync,
    TP: TokenProvider + Send + Sync,
{
    async fn get(
        &self,
        key: &(String, String),
    ) -> Result<
        Option<(registry::v1::Application, registry::v1::Device)>,
        ClientError<reqwest::Error>,
    > {
        Ok(
            match try_join!(self.get_app(&key.0,), self.get_device(&key.0, &key.1,),)? {
                (Some(app), Some(device)) => Some((app, device)),
                _ => None,
            },
        )
    }

    async fn update_if(
        &self,
        original: &registry::v1::Device,
        mut current: registry::v1::Device,
    ) -> Result<(), ReconcileError> {
        current.update_section(core::v1::Conditions::aggregate_ready)?;

        if original != &current {
            match self.update_device(&current).await {
                Ok(_) => Ok(()),
                Err(err) => match err {
                    ClientError::Syntax(msg) => Err(ReconcileError::permanent(format!(
                        "Failed to reconcile: {}",
                        msg
                    ))),
                    err => Err(ReconcileError::temporary(format!(
                        "Failed to reconcile: {}",
                        err
                    ))),
                },
            }
        } else {
            Ok(())
        }
    }

    fn ref_output(
        input: &(registry::v1::Application, registry::v1::Device),
    ) -> &registry::v1::Device {
        &input.1
    }
}
