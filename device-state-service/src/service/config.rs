use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresServiceConfiguration {
    pub pg: deadpool_postgres::Config,
}
