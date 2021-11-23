mod access_tokens;
pub mod apps;
pub mod devices;
mod overview;
mod token;

pub use access_tokens::*;
pub use overview::*;
pub use token::*;

use drogue_client::{core, Translator};
use patternfly_yew::*;
use yew::prelude::*;

pub trait HasReadyState {
    fn render_condition<S: AsRef<str>>(&self, name: S) -> Html {
        let name = name.as_ref();
        match self
            .conditions()
            .and_then(|c| c.0.into_iter().find(|c| c.r#type == name))
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

    fn render_state(&self) -> Html {
        self.render_condition("Ready")
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
