use crate::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use async_trait::async_trait;
use drogue_client::meta::v1::CommonMetadataMut;

pub struct HasFinalizer(pub &'static str);

pub trait MetadataContext {
    fn as_metadata_mut(&mut self) -> &mut dyn CommonMetadataMut;
}

#[async_trait]
impl<C> ProgressOperation<C> for HasFinalizer
where
    C: MetadataContext + Send + Sync + 'static,
{
    fn type_name(&self) -> String {
        "HasFinalizer".into()
    }

    async fn run(&self, mut ctx: C) -> Result<OperationOutcome<C>, ReconcileError> {
        // ensure we have a finalizer
        if ctx.as_metadata_mut().ensure_finalizer(self.0) {
            // early return
            Ok(OperationOutcome::Retry(ctx, None))
        } else {
            Ok(OperationOutcome::Continue(ctx))
        }
    }
}
