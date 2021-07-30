use crate::controller::base::{ProcessOutcome, StatusSection};
use crate::controller::reconciler::ReconcileError;
use async_trait::async_trait;
use drogue_client::core::v1::{ConditionStatus, Conditions};
use std::{future::Future, time::Duration};

pub struct Constructor<'c, C>(Vec<Box<dyn ConstructOperation<C> + 'c>>);

pub enum Outcome<C>
where
    C: Send + Sync,
{
    Continue(C),
    Retry(C, Option<Duration>),
}

pub type Result<T> = std::result::Result<Outcome<T>, ReconcileError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Construction<C> {
    Complete(C, Conditions),
    Retry(C, Option<Duration>, Conditions),
    Failed(ReconcileError, Conditions),
}

impl<'c, C> Constructor<'c, C>
where
    C: Send + Sync,
{
    pub fn new(steps: Vec<Box<dyn ConstructOperation<C> + 'c>>) -> Self {
        Self(steps)
    }

    pub async fn run(&self, mut conditions: Conditions, mut context: C) -> Construction<C> {
        let mut i = self.0.iter();

        while let Some(s) = i.next() {
            let condition_type = s.type_name();
            context = match s.run(context).await {
                Ok(Outcome::Continue(context)) => {
                    conditions.update(
                        condition_type,
                        ConditionStatus {
                            status: Some(true),
                            ..Default::default()
                        },
                    );
                    context
                }
                Ok(Outcome::Retry(mut context, when)) => {
                    conditions.update(
                        condition_type,
                        ConditionStatus {
                            status: Some(false),
                            ..Default::default()
                        },
                    );
                    while let Some(s) = i.next() {
                        let condition_type = s.type_name();
                        let (c, status) = s.when_skipped(context);
                        conditions.update(condition_type, status);
                        context = c;
                    }
                    return Construction::Retry(context, when, conditions);
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
                    while let Some(s) = i.next() {
                        let condition_type = s.type_name();
                        let status = s.when_failed();
                        conditions.update(condition_type, status);
                    }
                    return Construction::Failed(err, conditions);
                }
            }
        }

        Construction::Complete(context, conditions)
    }
}

#[async_trait]
pub trait ConstructOperation<C>: Send + Sync
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
impl<S, F, Fut, C> ConstructOperation<C> for (S, F)
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

pub mod application {
    use super::RunConstructor;
    use crate::controller::{
        base::{ConditionExt, ProcessOutcome, ReadyState, StatusSection, CONDITION_RECONCILED},
        reconciler::{
            construct::{Construction, Constructor},
            ReconcileError,
        },
    };
    use async_trait::async_trait;
    use drogue_client::{core, registry};

    pub trait ApplicationAccessor {
        fn app(&self) -> &registry::v1::Application;
        fn app_mut(&mut self) -> &mut registry::v1::Application;
        fn into(self) -> registry::v1::Application;
        fn conditions(&self) -> core::v1::Conditions;
    }

    #[async_trait]
    impl<'c, C> RunConstructor for Constructor<'c, C>
    where
        C: ApplicationAccessor + Send + Sync,
    {
        type Context = C;
        type Output = registry::v1::Application;

        async fn run_with<S>(&self, ctx: C) -> Result<ProcessOutcome<Self::Output>, ReconcileError>
        where
            S: StatusSection,
        {
            let observed_generation = ctx.app().metadata.generation;
            let mut original_app = ctx.app().clone();
            let conditions = ctx.conditions();

            let result = match self.run(conditions, ctx).await {
                Construction::Complete(mut context, mut conditions) => {
                    conditions.update(CONDITION_RECONCILED, ReadyState::Complete);
                    context
                        .app_mut()
                        .finish_ready::<S>(conditions, observed_generation)?;
                    ProcessOutcome::Complete(context.into())
                }
                Construction::Retry(mut context, when, mut conditions) => {
                    conditions.update(CONDITION_RECONCILED, ReadyState::Progressing);
                    context
                        .app_mut()
                        .finish_ready::<S>(conditions, observed_generation)?;
                    ProcessOutcome::Retry(context.into(), when)
                }
                Construction::Failed(err, mut conditions) => {
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
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{DateTime, Utc};
    use drogue_client::core::v1::Condition;

    fn set_now<C>(now: DateTime<Utc>, result: &mut Construction<C>) {
        match result {
            Construction::Complete(_, c) => {
                c.0.iter_mut().for_each(|c| c.last_transition_time = now)
            }
            Construction::Retry(_, _, c) => {
                c.0.iter_mut().for_each(|c| c.last_transition_time = now)
            }
            Construction::Failed(_, c) => c.0.iter_mut().for_each(|c| c.last_transition_time = now),
        }
    }

    #[tokio::test]
    async fn test_single() {
        #[derive(Debug, PartialEq, Eq)]
        struct Context {}

        let conditions = Conditions::default();

        let c = Constructor::<Context>(vec![Box::new(("Foo", |ctx| async {
            println!("Foo");
            Ok(Outcome::Continue(ctx))
        }))]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Construction::Complete(
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

        let c = Constructor::<Context>(vec![
            Box::new(("Foo", |ctx| async {
                println!("Foo");
                Ok(Outcome::Continue(ctx))
            })),
            Box::new(("Bar", |ctx| async {
                println!("Bar");
                Ok(Outcome::Retry(ctx, None))
            })),
            Box::new(("Baz", |ctx| async {
                println!("Baz");
                Ok(Outcome::Continue(ctx))
            })),
        ]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Construction::Retry(
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

        let c = Constructor::<Context>(vec![
            Box::new(("Foo", |ctx| async {
                println!("Foo");
                Ok(Outcome::Continue(ctx))
            })),
            Box::new(("Bar", |_| async {
                println!("Bar");
                Err(ReconcileError::permanent("Some error"))
            })),
            Box::new(("Baz", |ctx| async {
                println!("Baz");
                Ok(Outcome::Continue(ctx))
            })),
        ]);

        let mut result = c.run(conditions, Context {}).await;

        // align times
        let now = Utc::now();
        set_now(now, &mut result);

        assert_eq!(
            result,
            Construction::Failed(
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
