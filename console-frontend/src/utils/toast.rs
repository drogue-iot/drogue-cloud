use patternfly_yew::*;
use std::time::Duration;
use yew::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct ToastBuilder {
    r#type: Type,
    title: Option<String>,
    timeout: Option<Duration>,
    body: Option<Html>,
}

impl ToastBuilder {
    pub fn new() -> Self {
        ToastBuilder::default()
    }

    pub fn success() -> Self {
        Self {
            r#type: Type::Success,
            ..Default::default()
        }
    }

    pub fn title<S: Into<String>>(mut self, title: S) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn body<M: Into<Html>>(mut self, body: M) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn message<M: Into<Html>>(mut self, msg: M) -> Self {
        self.body = Some(html! {
            <Content><p>{ msg.into() }</p></Content>
        });
        self
    }
}

impl From<ToastBuilder> for Toast {
    fn from(b: ToastBuilder) -> Self {
        Toast {
            title: b.title.unwrap_or_else(|| "<<missing title>>".into()),
            r#type: b.r#type,
            timeout: b.timeout,
            body: b.body.unwrap_or_default(),
            ..Default::default()
        }
    }
}

pub trait ToastMessage {
    fn into_html(self) -> Html;
}

impl ToastMessage for String {
    fn into_html(self) -> Html {
        return html! {<Content><p>{ self }</p></Content>};
    }
}

impl ToastMessage for &str {
    fn into_html(self) -> Html {
        return html! {<Content><p>{ self.to_string() }</p></Content>};
    }
}

impl ToastMessage for Html {
    fn into_html(self) -> Html {
        self
    }
}

pub fn success<M: ToastMessage>(body: M) {
    ToastBuilder::success()
        .title("Success")
        .body(body.into_html())
        .timeout(Duration::from_secs(3))
        .toast();
}
