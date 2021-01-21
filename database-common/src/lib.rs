pub mod error;
pub mod models;

use async_trait::async_trait;
use deadpool::managed::Object;
use deadpool_postgres::ClientWrapper;
use std::ops::Deref;
use tokio_postgres::{types::ToSql, Error, Row, Statement, ToStatement, Transaction};

#[async_trait]
pub trait Client: Sync {
    async fn prepare(&self, query: &str) -> Result<Statement, Error>;

    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + Sync;

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + Sync;
}

#[async_trait]
impl Client for Object<ClientWrapper, Error> {
    async fn prepare(&self, query: &str) -> Result<Statement, Error> {
        self.deref().prepare(query).await
    }

    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().execute(statement, params).await
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().query_opt(statement, params).await
    }
}

#[async_trait]
impl Client for ClientWrapper {
    async fn prepare(&self, query: &str) -> Result<Statement, Error> {
        self.deref().prepare(query).await
    }

    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().execute(statement, params).await
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().query_opt(statement, params).await
    }
}

#[async_trait]
impl<'a> Client for Transaction<'a> {
    async fn prepare(&self, query: &str) -> Result<Statement, Error> {
        Transaction::prepare(self, query).await
    }

    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        Transaction::execute(self, statement, params).await
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        Transaction::query_opt(&self, statement, params).await
    }
}

#[async_trait]
impl<'a> Client for deadpool_postgres::Transaction<'a> {
    async fn prepare(&self, query: &str) -> Result<Statement, Error> {
        deadpool_postgres::Transaction::prepare(self, query).await
    }

    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().execute(statement, params).await
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + Sync,
    {
        self.deref().query_opt(statement, params).await
    }
}
