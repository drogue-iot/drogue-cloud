mod create;
mod details;
mod index;
pub mod ownership;

pub use create::*;
pub use details::*;
pub use index::*;

use crate::console::AppRoute;
use std::fmt::Formatter;
use std::str::FromStr;
use yew_nested_router::prelude::*;

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum Pages {
    // #[to = "/{name}/{*:details}"]
    Details {
        name: String,
        #[target(nested)]
        details: DetailsSection,
    },
    #[target(index)]
    Index,
}

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum DetailsSection {
    Integrations,
    Yaml,
    Debug,
    Administration,
    #[target(index)]
    Overview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationContext {
    Any,
    Single(String),
}

impl Default for ApplicationContext {
    fn default() -> Self {
        Self::Any
    }
}

impl core::fmt::Display for ApplicationContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Any => Ok(()),
            Self::Single(app) => f.write_str(app),
        }
    }
}

impl FromStr for ApplicationContext {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s.is_empty() {
            Self::Any
        } else {
            Self::Single(s.to_string())
        })
    }
}
