use crate::controller::{
    base::{ConditionExt, ProcessOutcome, ReadyState, StatusSection, CONDITION_RECONCILED},
    reconciler::ReconcileError,
};
use async_trait::async_trait;
use drogue_client::{
    core::{
        self,
        v1::{ConditionStatus, Conditions},
    },
    meta::v1::{CommonMetadata, CommonMetadataMut},
    Translator,
};
use std::{
    fmt::{Debug, Formatter},
    future::Future,
    time::Duration,
};
use tracing::instrument;

pub struct Progressor<'c, C>(Vec<Box<dyn ProgressOperation<C> + 'c>>);

pub enum OperationOutcome<C>
where
    C: Send + Sync,
{
    Continue(C),
    Retry(C, Option<Duration>),
}

impl<C> Debug for OperationOutcome<C>
where
    C: Send + Sync,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Continue(_) => f.debug_tuple("Continue").field(&"...").finish(),
            Self::Retry(_, dur) => f.debug_tuple("Retry").field(&"...").field(dur).finish(),
        }
    }
}

pub type Result<T> = std::result::Result<OperationOutcome<T>, ReconcileError>;

#[derive(Clone, PartialEq, Eq)]
pub enum Progress<C> {
    Complete(C, Conditions),
    Retry(C, Option<Duration>, Conditions),
    Failed(ReconcileError, Conditions),
}

impl<C> Debug for Progress<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete(_, _) => f.debug_tuple("Complete").finish(),
            Self::Retry(_, delay, _) => f.debug_tuple("Retry").field(&delay).finish(),
            Self::Failed(err, _) => f.debug_tuple("Failed").field(&err).finish(),
        }
    }
}

impl<'c, C> Progressor<'c, C>
where
    C: Send + Sync,
{
    pub fn new(steps: Vec<Box<dyn ProgressOperation<C> + 'c>>) -> Self {
        Self(steps)
    }

    #[instrument(skip(self, conditions, context), ret)]
    pub async fn run(&self, mut conditions: Conditions, mut context: C) -> Progress<C> {
        let mut i = self.0.iter();

        while let Some(s) = i.next() {
            let condition_type = s.type_name();
            let result = s.run(context).await;

            log::debug!("Progressing ({}): {:?}", condition_type, result);

            context = match result {
                Ok(OperationOutcome::Continue(context)) => {
                    conditions.update(
                        condition_type,
                        ConditionStatus {
                            status: Some(true),
                            ..Default::default()
                        },
                    );
                    context
                }
                Ok(OperationOutcome::Retry(mut context, when)) => {
                    conditions.update(
                        condition_type,
                        ConditionStatus {
                            status: Some(false),
                            ..Default::default()
                        },
                    );
                    for s in i {
                        let condition_type = s.type_name();
                        let (c, status) = s.when_skipped(context);
                        conditions.update(condition_type, status);
                        context = c;
                    }
                    return Progress::Retry(context, when, conditions);
                }
                Err(err) => {
                    conditions.update(
                        condition_type,
                        ConditionStatus {
                            status: None,
                            reason: Some("Failed".into()),
                            message: Some(err.to_string()),
                        },
                    );
                    for s in i {
                        let condition_type = s.type_name();
                        let status = s.when_failed();
                        conditions.update(condition_type, status);
                    }
                    return Progress::Failed(err, conditions);
                }
            }
        }

        Progress::Complete(context, conditions)
    }
}

#[async_trait]
pub trait ProgressOperation<C>: Send + Sync
where
    C: Send + Sync,
{
    fn type_name(&self) -> String;

    async fn run(&self, context: C) -> Result<C>;

    fn when_skipped(&self, context: C) -> (C, ConditionStatus) {
        (context, ConditionStatus::default())
    }

    fn when_failed(&self) -> ConditionStatus {
        ConditionStatus::default()
    }
}

#[async_trait]
impl<S, F, Fut, C> ProgressOperation<C> for (S, F)
where
    S: ToString + Send + Sync,
    F: Fn(C) -> Fut + Send + Sync,
    Fut: Future<Output = Result<C>> + Send + Sync,
    C: Send + Sync + 'static,
{
    fn type_name(&self) -> String {
        self.0.to_string()
    }

    async fn run(&self, context: C) -> Result<C> {
        self.1(context).await
    }
}

#[async_trait]
pub trait RunConstructor {
    type Context;
    type Output;

    async fn run_with<S: StatusSection>(
        &self,
        ctx: Self::Context,
    ) -> std::result::Result<ProcessOutcome<Self::Output>, ReconcileError>;
}

pub trait ResourceAccessor {
    type Resource: Clone
        + Translator
        + AsRef<dyn CommonMetadata>
        + AsMut<dyn CommonMetadataMut>
        + Send
        + Sync;
    fn resource(&self) -> &Self::Resource;
    fn resource_mut(&mut self) -> &mut Self::Resource;
    fn into(self) -> Self::Resource;
    fn conditions(&self) -> core::v1::Conditions;
}

#[async_trait]
impl<'c, C> RunConstructor for Progressor<'c, C>
where
    C: ResourceAccessor + Send + Sync,
{
    type Context = C;
    type Output = C::Resource;

    async fn run_with<S>(
        &self,
        ctx: C,
    ) -> std::result::Result<ProcessOutcome<Self::Output>, ReconcileError>
    where
        S: StatusSection,
    {
        let observed_generation = ctx.resource().as_ref().generation();
        let mut original_app = ctx.resource().clone();
        let conditions = ctx.conditions();

        let result = match self.run(conditions, ctx).await {
            Progress::Complete(mut context, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Complete);
                context
                    .resource_mut()
                    .finish_ready::<S>(conditions, observed_generation)?;
                ProcessOutcome::Complete(context.into())
            }
            Progress::Retry(mut context, when, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Progressing);
                context
                    .resource_mut()
                    .finish_ready::<S>(conditions, observed_generation)?;
                ProcessOutcome::Retry(context.into(), when)
            }
            Progress::Failed(err, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Failed(err.to_string()));
                original_app.finish_ready::<S>(conditions, observed_generation)?;
                match err {
                    ReconcileError::Permanent(_) => ProcessOutcome::Complete(original_app),
                    ReconcileError::Temporary(_) => ProcessOutcome::Retry(original_app, None),
                }
            }
        };

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{DateTime, Utc};
    use drogue_client::core::v1::Condition;

    fn set_now<C>(now: DateTime<Utc>, result: &mut Progress<C>) {
        match result {
            Progress::Complete(_, c) => c.0.iter_mut().for_each(|c| c.last_transition_time = now),
            Progress::Retry(_, _, c) => c.0.iter_mut().for_each(|c| c.last_transition_time = now),
            Progress::Failed(_, c) => c.0.iter_mut().for_each(|c| c.last_transition_time = now),
        }
    }

    #[tokio::test]
    async fn test_single() {
        #[derive(Debug, PartialEq, Eq)]
        struct Context {}

        let conditions = Conditions::default();

        let c = Progressor::<Context>(vec![Box::new(("Foo", |ctx| async {
            println!("Foo");
            Ok(OperationOutcome::Continue(ctx))
        }))]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Progress::Complete(
                Context {},
                Conditions(vec![Condition {
                    last_transition_time: now,
                    message: None,
                    reason: None,
                    status: "True".to_string(),
                    r#type: "Foo".to_string()
                }])
            )
        );
    }

    #[tokio::test]
    async fn test_multiple() {
        #[derive(Debug, PartialEq, Eq)]
        struct Context {}

        let conditions = Conditions::default();

        let c = Progressor::<Context>(vec![
            Box::new(("Foo", |ctx| async {
                println!("Foo");
                Ok(OperationOutcome::Continue(ctx))
            })),
            Box::new(("Bar", |ctx| async {
                println!("Bar");
                Ok(OperationOutcome::Retry(ctx, None))
            })),
            Box::new(("Baz", |ctx| async {
                println!("Baz");
                Ok(OperationOutcome::Continue(ctx))
            })),
        ]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Progress::Retry(
                Context {},
                None,
                Conditions(vec![
                    Condition {
                        last_transition_time: now,
                        message: None,
                        reason: None,
                        status: "True".to_string(),
                        r#type: "Foo".to_string()
                    },
                    Condition {
                        last_transition_time: now,
                        message: None,
                        reason: None,
                        status: "False".to_string(),
                        r#type: "Bar".to_string()
                    },
                    Condition {
                        last_transition_time: now,
                        message: None,
                        reason: None,
                        status: "Unknown".to_string(),
                        r#type: "Baz".to_string()
                    }
                ])
            )
        );
    }

    #[tokio::test]
    async fn test_multiple_fail() {
        #[derive(Debug, PartialEq, Eq)]
        struct Context {}

        let conditions = Conditions::default();

        let c = Progressor::<Context>(vec![
            Box::new(("Foo", |ctx| async {
                println!("Foo");
                Ok(OperationOutcome::Continue(ctx))
            })),
            Box::new(("Bar", |_| async {
                println!("Bar");
                Err(ReconcileError::permanent("Some error"))
            })),
            Box::new(("Baz", |ctx| async {
                println!("Baz");
                Ok(OperationOutcome::Continue(ctx))
            })),
        ]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Progress::Failed(
                ReconcileError::permanent("Some error"),
                Conditions(vec![
                    Condition {
                        last_transition_time: now,
                        message: None,
                        reason: None,
                        status: "True".to_string(),
                        r#type: "Foo".to_string()
                    },
                    Condition {
                        last_transition_time: now,
                        message: Some(
                            "Reconciliation failed with a permanent error: Some error".to_string()
                        ),
                        reason: Some("Failed".to_string()),
                        status: "Unknown".to_string(),
                        r#type: "Bar".to_string()
                    },
                    Condition {
                        last_transition_time: now,
                        message: None,
                        reason: None,
                        status: "Unknown".to_string(),
                        r#type: "Baz".to_string()
                    }
                ])
            )
        );
    }
}
