mod details;
mod index;

pub use details::*;
pub use index::*;

use crate::page::AppRoute;
use patternfly_yew::*;
use std::fmt::Formatter;
use std::str::FromStr;
use yew_router::prelude::*;

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum Pages {
    #[to = "/{name}/{*:details}"]
    Details {
        name: String,
        details: DetailsSection,
    },
    #[to = "/"]
    Index,
}

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum DetailsSection {
    #[to = "integrations"]
    Integrations,
    #[to = "yaml"]
    Yaml,
    #[end]
    Overview,
    #[to = "administration"]
    Administration,
}

pub type ApplicationTabs = TabsRouter<AppRoute, DetailsSection>;

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
