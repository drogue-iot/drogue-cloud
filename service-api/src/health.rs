use async_trait::async_trait;

#[async_trait]
pub trait HealthCheckedService {
    type HealthCheckError;

    async fn is_ready(&self) -> Result<(), Self::HealthCheckError>;
}
