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

#[derive(Debug, Clone, PartialEq, Eq, Target)]
pub enum Pages {
    Details {
        app: ApplicationContext,
        name: String,
        #[target(nested, default)]
        details: DetailsSection,
    },
   #[target(index)]
    Index { app: ApplicationContext },
}

#[derive(Debug, Clone, PartialEq, Eq, Target)]
pub enum DetailsSection {
    Yaml,
    Debug,
    Overview,
}

impl Default for DetailsSection {
  fn default() -> Self {
    Self::Overview
  }
}

pub type DevicesTabs = TabsRouter<DetailsSection>;
