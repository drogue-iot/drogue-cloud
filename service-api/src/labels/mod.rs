#[cfg(feature = "nom")]
mod parser;

#[cfg(feature = "nom")]
pub use parser::*;

#[cfg(feature = "nom")]
use std::convert::TryFrom;

#[derive(Default)]
pub struct LabelSelector(pub Vec<Operation>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Eq(String, String),
    NotEq(String, String),
    In(String, Vec<String>),
    NotIn(String, Vec<String>),
    Exists(String),
    NotExists(String),
}

#[cfg(feature = "nom")]
impl TryFrom<&str> for LabelSelector {
    type Error = parser::ParserError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(LabelSelector(parser::parse_from(value)?))
    }
}

#[cfg(feature = "nom")]
impl TryFrom<String> for LabelSelector {
    type Error = parser::ParserError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(LabelSelector(parser::parse_from(&value)?))
    }
}
