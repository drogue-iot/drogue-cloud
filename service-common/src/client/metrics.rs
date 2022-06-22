//! Metrics support for clients

use drogue_cloud_service_api::metrics::{AsPassFailError, PassFailError};
use prometheus::IntGaugeVec;

pub trait PassFailErrorExt {
    fn record_outcome(self, gauge: &IntGaugeVec) -> Self;
}

/// Record outcome to a gauge with the "outcome" label as first label.
impl<T: AsPassFailError> PassFailErrorExt for T {
    fn record_outcome(self, gauge: &IntGaugeVec) -> Self {
        match self.as_pass_fail_error() {
            PassFailError::Pass => gauge.with_label_values(&["pass"]).inc(),
            PassFailError::Fail => gauge.with_label_values(&["fail"]).inc(),
            PassFailError::Error => gauge.with_label_values(&["error"]).inc(),
        }
        self
    }
}
