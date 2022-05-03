use crate::controller::{base::ResourceOperations, reconciler::ReconcileError};
use async_trait::async_trait;
use drogue_client::{core, error::ClientError, registry, Translator};
use std::ops::Deref;

#[async_trait]
impl<S> ResourceOperations<String, registry::v1::Application, registry::v1::Application> for S
where
    S: Deref<Target = registry::v1::Client> + Send + Sync,
{
    async fn get(&self, key: &String) -> Result<Option<registry::v1::Application>, ClientError> {
        self.get_app(&key).await
    }

    async fn update_if(
        &self,
        original: &registry::v1::Application,
        mut current: registry::v1::Application,
    ) -> Result<(), ReconcileError> {
        current.update_section(core::v1::Conditions::aggregate_ready)?;

        if original != &current {
            match self.update_app(&current).await {
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

    fn ref_output(input: &registry::v1::Application) -> &registry::v1::Application {
        input
    }
}
