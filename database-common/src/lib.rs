pub mod error;
pub mod models;
pub mod utils;

use crate::error::ServiceError;
use async_trait::async_trait;
use deadpool::managed::Object;
use deadpool_postgres::{ClientWrapper, Pool};
use std::ops::Deref;
use tokio_postgres::types::BorrowToSql;
use tokio_postgres::{types::ToSql, Error, Row, RowStream, Statement, ToStatement, Transaction};

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

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Sync + Send,
        I::IntoIter: ExactSizeIterator;
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

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Sync + Send,
        I::IntoIter: ExactSizeIterator,
    {
        self.deref().query_raw(statement, params).await
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

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Sync + Send,
        I::IntoIter: ExactSizeIterator,
    {
        self.deref().query_raw(statement, params).await
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

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Sync + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Transaction::query_raw(&self, statement, params).await
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

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Sync + Send,
        I::IntoIter: ExactSizeIterator,
    {
        self.deref().query_raw(statement, params).await
    }
}

/// A database based service.
#[async_trait]
pub trait DatabaseService: Sync {
    fn pool(&self) -> &Pool;

    async fn is_ready(&self) -> Result<(), ServiceError> {
        self.pool().get().await?.simple_query("SELECT 1").await?;
        Ok(())
    }
}
