use patternfly_yew::*;
use yew::prelude::*;

use cloudevents::{event::Data, AttributesReader, Event};
use wasm_bindgen::{closure::Closure, JsValue};
use web_sys::{EventSource, EventSourceInit};

pub struct Spy {
    source: EventSource,
    events: Vec<Entry>,
}

use crate::Backend;

pub enum Msg {
    Event(Event),
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry(pub Event);

impl TableRenderer for Entry {
    fn render(&self, col: ColumnIndex) -> Html {
        match col.index {
            0 => render_timestamp(&self.0),
            1 => render_data_short(&self.0),
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
        let url = format!("{}/spy", Backend::get().unwrap().url);
        let source =
            EventSource::new_with_event_source_init_dict(&url, &EventSourceInit::new()).unwrap();

        let on_message = Closure::wrap(Box::new(move |msg: &JsValue| {
            let msg = extract_event(msg);
            link.send_message(msg);
        }) as Box<dyn FnMut(&JsValue)>);
        source.set_onmessage(Some(&on_message.into_js_value().into()));

        Self {
            events: Vec::new(),
            source,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Event(event) => {
                log::info!("Pushing event: {:?}", event);
                self.events.insert(0, Entry(event));
                while self.events.len() > 10 {
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
        let columns = vec![
            html_nested! {<TableColumn label="Timestamp (UTC)"/>},
            html_nested! {<TableColumn label="Payload"/>},
        ];

        log::info!("Columns: {:?}", columns);

        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Device Message Spy"}
                            <Popover
                                toggle_by_onclick=true
                                target=html!{<Button variant=Variant::Plain icon=Icon::Help align=Align::End></Button>}
                                header=html!{<Title size=Size::Medium>{"Data acquisition"}</Title>}
                                >
                                <div>
                                    { "The" } <em> {" message spy "} </em> { "will show the messages received by a system
                                    at the time of watching. Only new messages will be displayed." }
                                </div>
                            </Popover>
                        </h1>
                    </Content>
                </PageSection>
                <PageSection>
                    <Table<Entry>
                        entries=self.events.clone()
                        mode=TableMode::CompactExpandable
                        header={html_nested!{
                            <TableHeader>
                                <TableColumn label="Timestamp"/>
                                <TableColumn label="Payload"/>
                            </TableHeader>
                        }}
                        >
                    </Table<Entry>>
                </PageSection>
            </>
        };
    }

    fn destroy(&mut self) {
        self.source.close();
    }
}

fn extract_event(msg: &JsValue) -> Msg {
    web_sys::console::debug_2(&JsValue::from("event: "), msg);

    let data: String = js_sys::Reflect::get(msg, &JsValue::from("data"))
        .unwrap()
        .as_string()
        .unwrap();

    match serde_json::from_str(&data) {
        Ok(event) => Msg::Event(event),
        Err(e) => Msg::Error(e.to_string()),
    }
}

impl Spy {}

use unicode_segmentation::UnicodeSegmentation;

fn render_data(event: &Event) -> Html {
    // let data: Option<Data> = event.get_data();

    match event.get_data() {
        None => html! {},
        Some(Data::String(text)) => html! { <pre> {text} </pre> },
        Some(Data::Binary(blob)) => html! { <pre> {blob} </pre> },
        Some(Data::Json(value)) => html! { <pre> {value} </pre> },
    }
}

fn render_blob(blob: &[u8]) -> String {
    let max = blob.len().max(100);
    let ellipsis = if blob.len() > max { ", …" } else { "" };
    format!("[{}; {:02x?}{}]", blob.len(), &blob[0..max], ellipsis)
}

fn truncate_str(len: usize, string: String) -> String {
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
    let str = match event.get_data() {
        None => None,
        Some(Data::String(text)) => Some(truncate_str(200, text)),
        Some(Data::Binary(blob)) => Some(render_blob(&blob)),
        Some(Data::Json(value)) => Some(truncate_str(200, value.to_string())),
    };

    match str {
        Some(str) => html! { <pre>{str}</pre> },
        None => html! {},
    }
}

fn render_timestamp(event: &Event) -> Html {
    event
        .get_time()
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
            <Table<AttributeEntry>
                entries=attrs
                mode=TableMode::CompactNoBorders
                header=html_nested!{
                    <TableHeader>
                        <TableColumn label="Key"/>
                        <TableColumn label="Value"/>
                    </TableHeader>
                }
                >
            </Table<AttributeEntry>>

            <h3>{"Payload"}</h3>
            { render_data(event) }
        </>
    };
}
