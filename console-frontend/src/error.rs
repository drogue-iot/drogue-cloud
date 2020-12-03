use patternfly_yew::{Toast, ToastDispatcher, Type};
use yew::prelude::*;

pub fn error<S1, S2>(title: S1, description: S2)
where
    S1: ToString,
    S2: ToString,
{
    ToastDispatcher::default().toast(Toast {
        title: title.to_string(),
        body: html! {<p>{description.to_string()}</p>},
        r#type: Type::Danger,
        ..Default::default()
    });
}
