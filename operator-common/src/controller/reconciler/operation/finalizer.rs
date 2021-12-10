use crate::controller::reconciler::progress::ResourceAccessor;
use crate::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use async_trait::async_trait;

pub struct HasFinalizer(pub &'static str);

#[async_trait]
impl<C> ProgressOperation<C> for HasFinalizer
where
    C: ResourceAccessor + Send + Sync + 'static,
{
    fn type_name(&self) -> String {
        "HasFinalizer".into()
    }

    async fn run(&self, mut ctx: C) -> Result<OperationOutcome<C>, ReconcileError> {
        // ensure we have a finalizer
        if ctx.resource_mut().as_mut().ensure_finalizer(self.0) {
            // early return
            Ok(OperationOutcome::Retry(ctx, None))
        } else {
            Ok(OperationOutcome::Continue(ctx))
        }
    }
}
