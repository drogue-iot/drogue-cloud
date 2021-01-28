pub mod app;
pub mod device;

use std::{collections::HashMap, error::Error};
use tokio_postgres::{
    types::{Json, WasNull},
    Row,
};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct TypedAlias(pub String, pub String);

/// Convert the "LABELS" column of a row into the "labels" value, handling "null" values
/// by using the default value.
pub(crate) fn labels_to_map(row: &Row) -> Result<HashMap<String, String>, tokio_postgres::Error> {
    Ok(row
        .try_get::<_, Json<_>>("LABELS")
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
