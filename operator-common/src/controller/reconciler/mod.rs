mod error;
pub mod operation;
pub mod progress;

pub use error::*;

use crate::controller::base::ProcessOutcome;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use core::fmt::{Debug, Formatter};
use drogue_client::meta::v1::CommonMetadata;
use tracing::instrument;

pub enum ReconcileState<I, C, D> {
    Ignore(I),
    Construct(C),
    Deconstruct(D),
}

impl<I, C, D> Debug for ReconcileState<I, C, D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Ignore(..) => write!(f, "Ignore(..)"),
            Self::Construct(..) => write!(f, "Construct(..)"),
            Self::Deconstruct(..) => write!(f, "Deconstruct(..)"),
        }
    }
}

pub trait EvalMetadata {
    fn finalizers(&self) -> &[String];
    fn deletion_timestamp(&self) -> &Option<DateTime<Utc>>;
}

impl<T> EvalMetadata for T
where
    T: AsRef<dyn CommonMetadata>,
{
    fn finalizers(&self) -> &[String] {
        self.as_ref().finalizers()
    }

    fn deletion_timestamp(&self) -> &Option<DateTime<Utc>> {
        self.as_ref().deletion_timestamp()
    }
}

/// Make it easier to reconcile an app, device combination "by device".
pub struct ByDevice<A, D>(pub A, pub D)
where
    D: AsRef<dyn CommonMetadata>;

impl<A, D> EvalMetadata for ByDevice<A, D>
where
    D: AsRef<dyn CommonMetadata>,
{
    fn finalizers(&self) -> &[String] {
        self.1.finalizers()
    }

    fn deletion_timestamp(&self) -> &Option<DateTime<Utc>> {
        self.1.deletion_timestamp()
    }
}

pub type ReconcileResult<T> = Result<T, ReconcileError>;

#[async_trait]
pub trait Reconciler {
    type Input;
    type Output;
    type Construct;
    type Deconstruct;

    async fn eval_state(
        &self,
        input: Self::Input,
    ) -> ReconcileResult<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>>;

    /// A default implementation for `eval_state`. When creation is requested
    /// (either by static program logic, or by e.g. the present of a spec section) the function will
    /// eval, based on the presence of the finalizer and the deletion timestamp, if the state is
    /// ignored, construct, or deconstruct.
    ///
    /// An implementor, using this function, should:
    /// * When constructing, first set the finalizer (and RetryNow), then perform all necessary operations.
    /// * When deconstructing, first perform all necessary options. At last, remove the finalizer.
    #[instrument(skip(ctx, construct, deconstruct, ignore), ret)]
    fn eval_by_finalizer<CTX, FC, FD, FI>(
        requested: bool,
        ctx: CTX,
        finalizer: &str,
        construct: FC,
        deconstruct: FD,
        ignore: FI,
    ) -> ReconcileResult<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>>
    where
        CTX: EvalMetadata,
        FC: FnOnce(CTX) -> Self::Construct,
        FD: FnOnce(CTX) -> Self::Deconstruct,
        FI: FnOnce(CTX) -> Self::Output,
    {
        let configured = ctx.finalizers().iter().any(|f| f == finalizer);
        let deleted = ctx.deletion_timestamp().is_some();

        Ok(match (requested, configured, deleted) {
            (false, false, _) => ReconcileState::Ignore(ignore(ctx)),
            (false, true, _) => ReconcileState::Deconstruct(deconstruct(ctx)),
            (true, _, false) => ReconcileState::Construct(construct(ctx)),
            (_, _, true) => ReconcileState::Deconstruct(deconstruct(ctx)),
        })
    }

    async fn construct(
        &self,
        c: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError>;
    async fn deconstruct(
        &self,
        d: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError>;
}

pub struct ReconcileProcessor<R>(pub R)
where
    R: Reconciler;

impl<R> ReconcileProcessor<R>
where
    R: Reconciler,
{
    #[instrument(skip_all, ret)]
    pub async fn reconcile(
        &self,
        input: R::Input,
    ) -> Result<ProcessOutcome<R::Output>, ReconcileError> {
        let state = self.0.eval_state(input).await?;
        log::debug!("Reconcile state: {:?}", state);
        match state {
            ReconcileState::Ignore(output) => Ok(ProcessOutcome::Complete(output)),
            ReconcileState::Construct(ctx) => self.0.construct(ctx).await,
            ReconcileState::Deconstruct(ctx) => self.0.deconstruct(ctx).await,
        }
    }
}
