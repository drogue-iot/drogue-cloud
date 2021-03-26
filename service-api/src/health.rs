use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HealthCheckError {
    #[error("Health check failed: {0}")]
    Failed(#[from] Box<dyn std::error::Error>),
}

impl HealthCheckError {
    pub fn from<E>(err: E) -> Self
    where
        E: std::error::Error + 'static,
    {
        HealthCheckError::Failed(Box::new(err))
    }
}

#[async_trait]
pub trait HealthChecked: Send + Sync {
    async fn is_ready(&self) -> Result<(), HealthCheckError> {
        Ok(())
    }

    async fn is_alive(&self) -> Result<(), HealthCheckError> {
        Ok(())
    }
}

pub trait AsHealthChecked {
    fn into_health_check(self) -> Box<dyn HealthChecked>;
}
