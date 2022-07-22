mod create;
mod delete;
mod details;
mod index;

pub use create::*;
pub use details::*;
pub use index::*;

use crate::console::AppRoute;
use crate::pages::apps::ApplicationContext;
use patternfly_yew::*;
use yew_router::prelude::*;

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum Pages {
    #[to = "/{app}/{name}/{*:details}"]
    Details {
        app: ApplicationContext,
        name: String,
        details: DetailsSection,
    },
    #[to = "/{app}/"]
    Index { app: ApplicationContext },
}

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum DetailsSection {
    #[to = "yaml"]
    Yaml,
    #[end]
    Overview,
}

pub type DevicesTabs = TabsRouter<AppRoute, DetailsSection>;
