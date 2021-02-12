use crate::{backend::Backend, error::error};
use cloudevents::{
    event::{Data, ExtensionValue},
    AttributesReader, Event,
};
use drogue_cloud_service_api::{EXT_APPLICATION, EXT_DEVICE};
use itertools::Itertools;
use patternfly_yew::*;
use unicode_segmentation::UnicodeSegmentation;
use wasm_bindgen::{closure::Closure, JsValue};
use web_sys::{EventSource, EventSourceInit, HtmlInputElement};
use yew::prelude::*;

pub struct Spy {
    link: ComponentLink<Self>,
    source: Option<EventSource>,
    events: SharedTableModel<Entry>,

    app_id_ref: NodeRef,

    running: bool,
    total_received: usize,
}

pub enum Msg {
    Start(Option<String>),
    StartPressed,
    Event(Box<Event>),
    /// Failed when processing an event
    Error(String),
    /// Source failed
    Failed,
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
            1 => self.device().into(),
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

impl Entry {
    fn device(&self) -> String {
        let app_id = self.extension_as_string(EXT_APPLICATION);
        let device_id = self.extension_as_string(EXT_DEVICE);

        format!("{} / {}", app_id, device_id)
    }

    fn extension_as_string(&self, name: &str) -> String {
        self.0
            .extension(name)
            .map(|s| match s {
                ExtensionValue::String(s) => s.clone(),
                ExtensionValue::Integer(i) => i.to_string(),
                ExtensionValue::Boolean(true) => "true".into(),
                ExtensionValue::Boolean(false) => "false".into(),
            })
            .unwrap_or_default()
    }
}

impl Component for Spy {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            events: Default::default(),
            link,
            source: None,
            running: false,
            app_id_ref: NodeRef::default(),
            total_received: 0,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Start(app_id) => {
                log::info!("Starting: {:?}", app_id);
                self.start(app_id);
            }
            Msg::StartPressed => {
                self.link.send_message(Msg::Start(self.app_id_filter()));
            }
            Msg::Event(event) => {
                // log::debug!("Pushing event: {:?}", event);
                self.total_received += 1;
                self.events.insert(0, Entry(*event));
                while self.events.len() > DEFAULT_MAX_SIZE {
                    self.events.pop();
                }
            }
            Msg::Error(err) => {
                error("Failed to process event", err);
            }
            Msg::Failed => {
                error("Source error", "Failed to connect to the event source");
                self.running = false;
            }
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
                        <Title>{"Device Message Spy"}</Title>
                    </Content>
                </PageSection>
                <PageSection>

                    <Toolbar>
                        <ToolbarGroup>
                            <ToolbarItem>
                                <TextInput
                                    ref=self.app_id_ref.clone()
                                    disabled=self.running
                                    placeholder="Application ID to spy on"/>
                            </ToolbarItem>
                            <ToolbarItem>
                                <Button
                                    disabled=self.running
                                    label="Start" icon=Icon::Play variant=Variant::Primary
                                    onclick=self.link.callback(|_|Msg::StartPressed)
                                    />
                            </ToolbarItem>
                        </ToolbarGroup>
                        <ToolbarItem modifiers=vec![ToolbarElementModifier::Right.all()]>
                            <strong>{"events received: "}{self.total_received}</strong>
                        </ToolbarItem>
                    </Toolbar>

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

                    { if self.events.is_empty() {
                        self.render_empty()
                    } else {
                        html!{}
                    }}
                </PageSection>
            </>
        };
    }

    fn destroy(&mut self) {
        if let Some(source) = &self.source {
            source.close();
        }
    }
}

impl Spy {
    fn app_id_filter(&self) -> Option<String> {
        let input = self.app_id_ref.cast::<HtmlInputElement>();
        log::info!("Input: {:?}", input);
        if let Some(input) = input {
            let value = input.value();
            log::info!("Input value: '{}'", value);
            match value.is_empty() {
                true => None,
                false => Some(value),
            }
        } else {
            None
        }
    }

    fn start(&mut self, app_id: Option<String>) {
        let mut url = Backend::url("/spy").unwrap();

        // add optional filter

        if let Some(app_id) = &app_id {
            url.query_pairs_mut().append_pair("app", app_id);
        }

        // EventSource doesn't support passing headers, so we cannot send
        // the bearer token the normal way

        url.query_pairs_mut()
            .append_pair("token", &Backend::access_token().unwrap_or_default());

        // create source

        let source =
            EventSource::new_with_event_source_init_dict(&url.to_string(), &EventSourceInit::new())
                .unwrap();

        // setup onmessage

        let link = self.link.clone();
        let on_message = Closure::wrap(Box::new(move |msg: &JsValue| {
            let msg = extract_event(msg);
            link.send_message(msg);
        }) as Box<dyn FnMut(&JsValue)>);
        source.set_onmessage(Some(&on_message.into_js_value().into()));

        // setup onerror

        let link = self.link.clone();
        let on_error = Closure::wrap(Box::new(move || {
            link.send_message(Msg::Failed);
        }) as Box<dyn FnMut()>);
        source.set_onerror(Some(&on_error.into_js_value().into()));

        // store result

        self.running = true; // FIXME: need a way to stop
        self.source = Some(source);
    }

    fn render_empty(&self) -> Html {
        return html! {
            <div style="padding-bottom: 10rem; height: 100%;">
            <Bullseye>
            <EmptyState
                title="No new messages"
                icon=Icon::Pending
                size=Size::XLarge
                >
                { "The " } <q> {"message spy"} </q> { " will only show "} <strong> {"new"} </strong> {" messages received by the system.
                When the next message arrives, you will see it right here." }
            </EmptyState>
            </Bullseye>
            </div>
        };
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
        Some(Data::Binary(blob)) => html! { <>
        <pre> { pretty_hex::pretty_hex(&blob) } </pre>
        <pre> { base64_block(&blob) } </pre>
        </> },
        Some(Data::Json(value)) => {
            let value = serde_json::to_string_pretty(&value).unwrap();
            return html! { <pre> {value} </pre> };
        }
    }
}

fn base64_block(input: &[u8]) -> String {
    base64::encode(input)
        .chars()
        .collect::<Vec<_>>()
        .chunks(120)
        .map(|chunk| chunk.iter().collect::<String>())
        .join("\n")
}

fn render_blob(blob: &[u8]) -> String {
    let max = blob.len().min(25);
    let ellipsis = if blob.len() > max { ", …" } else { "" };
    format!("[{}; {:02x?}{}]", blob.len(), &blob[0..max], ellipsis)
}

fn truncate_str(len: usize, string: &str) -> String {
    let mut r = String::new();
    for c in string.graphemes(true) {
        if r.len() > len || r.contains('\n') || r.contains('\r') {
            r.push('…');
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
