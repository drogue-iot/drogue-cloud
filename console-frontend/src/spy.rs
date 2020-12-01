use patternfly_yew::*;
use yew::prelude::*;

use cloudevents::{
    event::{Data, ExtensionValue},
    AttributesReader, Event,
};

use unicode_segmentation::UnicodeSegmentation;

use wasm_bindgen::{closure::Closure, JsValue};
use web_sys::{EventSource, EventSourceInit};

use crate::backend::Backend;

pub struct Spy {
    source: EventSource,
    events: SharedTableModel<Entry>,
}

pub enum Msg {
    Event(Event),
    Error(String),
}

const DEFAULT_MAX_SIZE: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry(pub Event);

impl TableRenderer for Entry {
    fn render(&self, col: ColumnIndex) -> Html {
        match col.index {
            // timestamp
            0 => render_timestamp(&self.0),
            // device id
            1 => self
                .0
                .extension("device_id")
                .map(|s| match s {
                    ExtensionValue::String(s) => s.clone(),
                    ExtensionValue::Integer(i) => i.to_string(),
                    ExtensionValue::Boolean(true) => "true".into(),
                    ExtensionValue::Boolean(false) => "false".into(),
                })
                .unwrap_or_default()
                .into(),
            // payload
            2 => render_data_short(&self.0),
            // ignore
            _ => html! {},
        }
    }

    fn render_details(&self) -> Vec<Span> {
        vec![Span::max(render_details(&self.0))]
    }
}

impl Component for Spy {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut url = Backend::url("/spy").unwrap();

        // EventSource doesn't support passing headers, so we cannot send
        // the bearer token the normal way

        url.query_pairs_mut()
            .append_pair("token", &Backend::access_token().unwrap_or_default());
        let source =
            EventSource::new_with_event_source_init_dict(&url.to_string(), &EventSourceInit::new())
                .unwrap();

        let on_message = Closure::wrap(Box::new(move |msg: &JsValue| {
            let msg = extract_event(msg);
            link.send_message(msg);
        }) as Box<dyn FnMut(&JsValue)>);
        source.set_onmessage(Some(&on_message.into_js_value().into()));

        Self {
            events: Default::default(),
            source,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Event(event) => {
                log::debug!("Pushing event: {:?}", event);
                self.events.insert(0, Entry(event));
                while self.events.len() > DEFAULT_MAX_SIZE {
                    self.events.pop();
                }
            }
            Msg::Error(_) => {}
        }
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Device Message Spy"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { if self.events.len() > 0 {
                        html!{
                            <Table<SharedTableModel<Entry>>
                                entries=self.events.clone()
                                mode=TableMode::CompactExpandable
                                header={html_nested!{
                                    <TableHeader>
                                        <TableColumn label="Timestamp (UTC)"/>
                                        <TableColumn label="Device ID"/>
                                        <TableColumn label="Payload"/>
                                    </TableHeader>
                                }}
                                >
                            </Table<SharedTableModel<Entry>>>
                        }
                    } else {
                        html!{
                            <EmptyState
                                title="No new messages"
                                icon=Icon::Pending
                                size=Size::XLarge
                                >
                                { "The " } <q> {"message spy"} </q> { " will only show "} <strong> {"new"} </strong> {" messages received by the system.
                                When the next message arrives, you will see it right here." }
                            </EmptyState>
                        }
                    }}
                </PageSection>
            </>
        };
    }

    fn destroy(&mut self) {
        self.source.close();
    }
}

fn extract_event(msg: &JsValue) -> Msg {
    // web_sys::console::debug_2(&JsValue::from("event: "), msg);

    let data: String = js_sys::Reflect::get(msg, &JsValue::from("data"))
        .unwrap()
        .as_string()
        .unwrap();

    match serde_json::from_str(&data) {
        Ok(event) => Msg::Event(event),
        Err(e) => Msg::Error(e.to_string()),
    }
}

fn render_data(event: &Event) -> Html {
    // let data: Option<Data> = event.get_data();

    match event.data() {
        None => html! {},
        Some(Data::String(text)) => html! { <pre> {text} </pre> },
        Some(Data::Binary(blob)) => html! { <pre> { format!("{:02x?}", blob) } </pre> },
        Some(Data::Json(value)) => {
            let value = serde_json::to_string_pretty(&value).unwrap();
            return html! { <pre> {value} </pre> };
        }
    }
}

fn render_blob(blob: &[u8]) -> String {
    let max = blob.len().min(50);
    let ellipsis = if blob.len() > max { ", …" } else { "" };
    format!("[{}; {:02x?}{}]", blob.len(), &blob[0..max], ellipsis)
}

fn truncate_str(len: usize, string: &str) -> String {
    let mut r = String::new();
    for c in string.graphemes(true) {
        if r.len() > len || r.contains('\n') || r.contains('\r') {
            r.push_str("…");
            break;
        }
        r.push_str(c);
    }
    r
}

fn render_data_short(event: &Event) -> Html {
    match event.data() {
        None => html! {},
        Some(Data::String(text)) => html! {
            <pre>
                <Label label="String" color=Color::Purple/>{" "}{truncate_str(100, text)}
            </pre>
        },
        Some(Data::Binary(blob)) => html! {
            <pre>
                <Label label="BLOB" color=Color::Blue/>{" "}{render_blob(&blob)}
            </pre>
        },
        Some(Data::Json(value)) => html! {
            <pre>
                <Label label="JSON" color=Color::Cyan/>{" "}{truncate_str(100, &value.to_string())}
            </pre>
        },
    }
}

fn render_timestamp(event: &Event) -> Html {
    event
        .time()
        .map(|ts| {
            return html! {
                <span>
                    <pre>{ts.format("%H:%M:%S%.3f %Y-%m-%d")}</pre>
                </span>
            };
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq)]
struct AttributeEntry(pub String, pub Html);

impl TableRenderer for AttributeEntry {
    fn render(&self, index: ColumnIndex) -> Html {
        match index.index {
            0 => html! {&self.0},
            1 => self.1.clone(),
            _ => html! {},
        }
    }
}

fn render_details(event: &Event) -> Html {
    let mut attrs: Vec<AttributeEntry> = event
        .iter()
        .map(|(key, value)| {
            (
                key.to_string(),
                html! {
                    <pre>{ value.to_string() }</pre>
                },
            )
        })
        .map(|(key, value)| AttributeEntry(key, value))
        .collect();

    attrs.sort_by(|a, b| a.0.cmp(&b.0));

    return html! {
        <>
            <h3>{"Attributes"}</h3>
            <Table<SimpleTableModel<AttributeEntry>>
                entries=SimpleTableModel::from(attrs)
                mode=TableMode::CompactNoBorders
                header=html_nested!{
                    <TableHeader>
                        <TableColumn label="Key"/>
                        <TableColumn label="Value"/>
                    </TableHeader>
                }
                >
            </Table<SimpleTableModel<AttributeEntry>>>

            <h3>{"Payload"}</h3>
            { render_data(event) }
        </>
    };
}
