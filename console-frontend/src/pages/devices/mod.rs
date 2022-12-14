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
use yew_nested_router::prelude::*;

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum Pages {
    Details {
        app: ApplicationContext,
        name: String,
        #[target(nested)]
        details: DetailsSection,
    },
    Index {
        app: ApplicationContext,
    },
}

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum DetailsSection {
    Yaml,
    Debug,
    #[target(index)]
    Overview,
}
