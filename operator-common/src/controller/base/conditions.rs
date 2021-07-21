use crate::controller::{base::CONDITION_RECONCILED, reconciler::ReconcileError};
use drogue_client::{
    core::v1::{ConditionStatus, Conditions},
    Dialect, Translator,
};
use serde::{Deserialize, Serialize};

pub enum ReadyState {
    Complete,
    Progressing,
    Failed(String),
}

impl From<ReadyState> for ConditionStatus {
    fn from(state: ReadyState) -> Self {
        match state {
            ReadyState::Complete => ConditionStatus {
                status: Some(true),
                reason: Some("AsExpected".into()),
                message: Some("".into()),
            },
            ReadyState::Progressing => ConditionStatus {
                status: Some(false),
                reason: Some("Progressing".into()),
                message: Some("".into()),
            },
            ReadyState::Failed(msg) => ConditionStatus {
                status: Some(false),
                reason: Some("Failed".into()),
                message: Some(msg),
            },
        }
    }
}

pub trait ConditionExt {
    fn eval_ready(conditions: &Conditions) -> Option<bool>;

    fn finish_ready<S: StatusSection>(
        &mut self,
        conditions: Conditions,
        observed_generation: u64,
    ) -> Result<(), ReconcileError>;

    fn set_status<S: StatusSection>(
        &mut self,
        conditions: Conditions,
        observed_generation: u64,
    ) -> Result<(), ReconcileError>;
}

impl<T> ConditionExt for T
where
    T: Translator,
{
    fn eval_ready(conditions: &Conditions) -> Option<bool> {
        let mut ready = Some(true);
        for condition in &conditions.0 {
            if condition.r#type == CONDITION_RECONCILED {
                continue;
            }
            match condition.status.as_str() {
                "True" => {}
                "False" => {
                    ready = Some(false);
                    break;
                }
                _ => {
                    ready = None;
                    break;
                }
            }
        }
        ready
    }

    fn finish_ready<S: StatusSection>(
        &mut self,
        conditions: Conditions,
        observed_generation: u64,
    ) -> Result<(), ReconcileError> {
        let ready = Self::eval_ready(&conditions);

        self.set_status::<S>(conditions, observed_generation)?;

        // update the global conditions sections

        let ready_state = ConditionStatus {
            status: ready,
            ..Default::default()
        };

        self.update_section(|mut conditions: Conditions| {
            conditions.update(S::ready_name(), ready_state);
            conditions
        })?;

        // done

        Ok(())
    }

    fn set_status<S: StatusSection>(
        &mut self,
        conditions: Conditions,
        observed_generation: u64,
    ) -> Result<(), ReconcileError> {
        self.update_section(|mut status: S| {
            status.update_status(conditions, observed_generation);
            status
        })?;
        Ok(())
    }
}

pub trait StatusSection: Serialize + for<'de> Deserialize<'de> + Dialect + Default {
    fn ready_name() -> &'static str;
    fn update_status(&mut self, conditions: Conditions, observed_generation: u64);
}
