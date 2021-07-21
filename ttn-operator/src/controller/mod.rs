pub mod app;
pub mod device;

use crate::data::TtnAppSpec;
use drogue_client::meta;
use drogue_cloud_operator_common::controller::reconciler::ReconcileError;

/// Ensure that the app ID did not change.
pub fn ensure_stable_app_id(
    meta: &meta::v1::NonScopedMetadata,
    spec: &TtnAppSpec,
    current_app_id: &str,
) -> Result<(), ReconcileError> {
    let defined_id = spec.api.id.as_ref().unwrap_or(&meta.name);
    if defined_id != current_app_id {
        Err(ReconcileError::permanent(format!(
            "Application IDs have changed - requested: {}, current: {}",
            defined_id, current_app_id
        )))
    } else {
        Ok(())
    }
}
