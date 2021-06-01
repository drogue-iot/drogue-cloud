mod details;
mod index;

pub use details::*;
pub use index::*;

use crate::page::AppRoute;
use patternfly_yew::*;
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
    #[to = "yaml"]
    Yaml,
    #[end]
    Overview,
}

pub type ApplicationTabs = TabsRouter<AppRoute, DetailsSection>;
