use web_sys::Node;
use yew::prelude::*;
use yew::virtual_dom::VNode;

pub trait ToHtml {
    fn to_html(&self) -> Html;
}

impl ToHtml for dyn AsRef<str> {
    fn to_html(&self) -> Html {
        let ele = yew::utils::document().create_element("div").unwrap();
        ele.set_inner_html(self.as_ref().into());

        VNode::VRef(Node::from(ele))
    }
}

impl ToHtml for String {
    fn to_html(&self) -> Html {
        let ele = yew::utils::document().create_element("div").unwrap();
        ele.set_inner_html(&self);

        VNode::VRef(Node::from(ele))
    }
}

pub fn url_encode<S: AsRef<str>>(s: S) -> String {
    percent_encoding::utf8_percent_encode(s.as_ref(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}
