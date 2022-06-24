use drogue_cloud_service_common::tls::ClientConfig;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub db: deadpool_postgres::Config,
    #[serde(default)]
    pub tls: ClientConfig,
}

impl Config {
    pub fn create_pool(&self) -> anyhow::Result<deadpool_postgres::Pool> {
        Ok(self.db.create_pool(
            Some(deadpool_postgres::Runtime::Tokio1),
            postgres_native_tls::MakeTlsConnector::new((&self.tls).try_into()?),
        )?)
    }
}
