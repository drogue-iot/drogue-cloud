pub mod app;
pub mod device;
pub mod diff;
mod gen;
pub mod outbox;
pub mod sql;

pub use gen::*;

use std::{collections::HashMap, error::Error, fmt};
use tokio_postgres::{
    row::RowIndex,
    types::{Json, WasNull},
    Row,
};
use uuid::Uuid;

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct TypedAlias(pub String, pub String);

#[derive(Clone, Debug)]
/// Constraints when modifying an object.
pub struct Constraints {
    pub uid: Uuid,
    pub resource_version: Uuid,
}

/// Convert a JSON map column from a row into a map value, handling "null" values
/// by using the default value.
pub(crate) fn row_to_map<I>(
    row: &Row,
    idx: I,
) -> Result<HashMap<String, String>, tokio_postgres::Error>
where
    I: RowIndex + fmt::Display,
{
    Ok(row
        .try_get::<_, Json<_>>(idx)
        .or_else(|err| fix_null(err, || Json(Default::default())))?
        .0)
}

/// Convert an array column from a row into a set value, handling "null" values
/// by using the default value.
pub(crate) fn row_to_vec<I>(row: &Row, idx: I) -> Result<Vec<String>, tokio_postgres::Error>
where
    I: RowIndex + fmt::Display,
{
    Ok(row
        .try_get::<_, Vec<String>>(idx)
        .or_else(fix_null_default)?)
}

/// Fix a null error by using an alternative value.
fn fix_null<T, F>(err: tokio_postgres::Error, f: F) -> Result<T, tokio_postgres::Error>
where
    F: FnOnce() -> T,
{
    err.source()
        .and_then(|e| match e.downcast_ref::<WasNull>() {
            Some(_) => Some(Ok(f())),
            None => None,
        })
        .unwrap_or(Err(err))
}

/// Fix a null value by using the default value.
fn fix_null_default<T>(err: tokio_postgres::Error) -> Result<T, tokio_postgres::Error>
where
    T: Default,
{
    fix_null(err, T::default)
}

pub enum Lock {
    None,
    ForUpdate,
    ForShare,
}

impl ToString for Lock {
    fn to_string(&self) -> String {
        self.as_ref().into()
    }
}

impl AsRef<str> for Lock {
    fn as_ref(&self) -> &str {
        match self {
            Self::None => "",
            Self::ForUpdate => "FOR UPDATE",
            Self::ForShare => "FOR SHARE",
        }
    }
}

pub trait Resource {
    fn resource_version(&self) -> Uuid;
    fn uid(&self) -> Uuid;
}

#[macro_export]
macro_rules! default_resource {
    ($name:ty) => {
        impl $crate::models::Resource for $name {
            fn resource_version(&self) -> Uuid {
                self.resource_version
            }
            fn uid(&self) -> Uuid {
                self.uid
            }
        }
    };
}

#[macro_export]
macro_rules! update_aliases {
    ($count:expr, $aliases:expr, |$a:ident| $code:block) => {{
        let count = $count;
        match (count > 0, $aliases) {
            // we found something, and need to update aliases
            (true, Some($a)) => $code,
            // we found something, but don't need to update aliases
            (true, None) => Ok(count),
            // we found nothing
            (false, _) => Err(ServiceError::NotFound),
        }
    }};
}
