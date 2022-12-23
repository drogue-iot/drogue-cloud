mod paging;
mod shell;
mod toast;
mod validators;
mod yaml;

pub use paging::*;
pub use shell::*;
pub use toast::*;
pub use validators::*;
pub use yaml::*;

use web_sys::Node;
use yew::prelude::*;
use yew::virtual_dom::VNode;
use yew_nested_router::prelude::*;

/// Macro to make it easier to use `html!` as value for a property.
///
/// ```rust
/// # use yew::prelude::*;
/// fn view() -> Html {
///    html!{
///        <Card
///            title={html_prop!({"Application Members"})}>
///        </Card>
///    }
/// }
/// ```
#[macro_export]
macro_rules! html_prop {
    ($html:tt) => {
        html! {$html}
    };
}

pub trait ToHtml {
    fn to_html(&self) -> Html;
}

impl ToHtml for dyn AsRef<str> {
    fn to_html(&self) -> Html {
        let ele = gloo_utils::document().create_element("div").unwrap();
        ele.set_inner_html(self.as_ref());

        VNode::VRef(Node::from(ele))
    }
}

impl ToHtml for String {
    fn to_html(&self) -> Html {
        let ele = gloo_utils::document().create_element("div").unwrap();
        ele.set_inner_html(self);

        VNode::VRef(Node::from(ele))
    }
}

pub fn url_encode<S: AsRef<str>>(s: S) -> String {
    percent_encoding::utf8_percent_encode(s.as_ref(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}

pub fn url_decode<S: AsRef<str>>(s: S) -> String {
    percent_encoding::percent_decode_str(s.as_ref())
        .decode_utf8_lossy()
        .to_string()
}

/// Navigate the router to the target.
pub fn navigate_to<SWITCH>(to: SWITCH)
where
    SWITCH: 'static + Target,
{
    use_router().unwrap().push(to);
}
