use prometheus::core::Collector;
use std::boxed::Box;

pub fn register(metric: Box<dyn Collector>) -> anyhow::Result<()> {
    match prometheus::default_registry().register(metric) {
        Ok(_) => Ok(()),
        Err(prometheus::Error::AlreadyReg) => {
            log::debug!("Metric already registered");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
