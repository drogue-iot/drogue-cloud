mod api_keys;
pub mod apps;
pub mod devices;
mod overview;
mod token;

pub use api_keys::*;
pub use overview::*;
pub use token::*;

use drogue_client::{core, Translator};
use patternfly_yew::*;
use yew::prelude::*;

pub trait HasReadyState {
    fn render_state(&self) -> Html {
        match self
            .conditions()
            .and_then(|c| c.0.into_iter().find(|c| c.r#type == "Ready"))
            .as_ref()
            .map(|s| s.status.as_str())
        {
            Some("True") => html! { <>
                {Icon::CheckCircle.with_state(State::Success)} <span>{" Ready"}</span>
            </> },
            Some("False") => html! { <>
                {Icon::ExclamationCircle.with_state(State::Danger)} <span>{" Not ready"}</span>
            </> },
            _ => html! { <>
                {Icon::QuestionCircle} <span>{" Unknown"}</span>
            </> },
        }
    }

    fn conditions(&self) -> Option<core::v1::Conditions>;
}

impl<T> HasReadyState for T
where
    T: Translator,
{
    fn conditions(&self) -> Option<core::v1::Conditions> {
        self.section().and_then(|s| s.ok())
    }
}
