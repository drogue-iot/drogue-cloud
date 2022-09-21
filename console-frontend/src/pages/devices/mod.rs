mod clone;
mod create;
mod debug;
mod delete;
mod details;
mod index;

pub use clone::*;
pub use create::*;
pub use debug::*;
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
    #[to = "debug"]
    Debug,
    #[end]
    Overview,
}

pub type DevicesTabs = TabsRouter<AppRoute, DetailsSection>;
