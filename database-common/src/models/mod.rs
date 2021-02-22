pub mod app;
pub mod device;

use std::{collections::HashMap, error::Error, fmt};
use tokio_postgres::{
    row::RowIndex,
    types::{Json, WasNull},
    Row,
};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct TypedAlias(pub String, pub String);

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
        .or_else(|err| {
            err.source()
                .and_then(|e| match e.downcast_ref::<WasNull>() {
                    Some(_) => Some(Ok(Json(Default::default()))),
                    None => None,
                })
                .unwrap_or(Err(err))
        })?
        .0)
}
