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
    fn eval_ready(conditions: &Conditions) -> Vec<String>;

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
    fn eval_ready(conditions: &Conditions) -> Vec<String> {
        let mut waiting = Vec::new();
        for condition in &conditions.0 {
            if condition.r#type == CONDITION_RECONCILED {
                continue;
            }
            match condition.status.as_str() {
                "True" => {}
                _ => {
                    waiting.push(condition.r#type.clone());
                    break;
                }
            }
        }
        waiting
    }

    fn finish_ready<S: StatusSection>(
        &mut self,
        conditions: Conditions,
        observed_generation: u64,
    ) -> Result<(), ReconcileError> {
        let waiting = Self::eval_ready(&conditions);

        self.set_status::<S>(conditions, observed_generation)?;

        // update the global conditions sections

        let ready_state = if waiting.is_empty() {
            ConditionStatus {
                status: Some(true),
                ..Default::default()
            }
        } else {
            let message = format!("Waiting to become ready: {}", waiting.join(", "));
            ConditionStatus {
                status: Some(false),
                reason: Some("WaitingForReady".into()),
                message: Some(message),
            }
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
