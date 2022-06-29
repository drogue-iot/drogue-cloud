use crate::{backend::BackendInformation, error::error, utils::not_empty};
use cloudevents::{
    event::{Data, ExtensionValue},
    AttributesReader, Event,
};
use drogue_client::integration::ws::v1::client;
use drogue_cloud_console_common::EndpointInformation;
use drogue_cloud_service_api::EXT_DEVICE;
use itertools::Itertools;
use patternfly_yew::*;
use unicode_segmentation::UnicodeSegmentation;
use url::Url;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{MessageEvent, WebSocket};
use yew::context::ContextHandle;
use yew::prelude::*;
use yew_oauth2::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: BackendInformation,
    pub endpoints: EndpointInformation,
    #[prop_or_default]
    pub application: Option<String>,
}

pub struct Spy {
    ws: Option<(
        WebSocket,
        Closure<dyn FnMut(&MessageEvent)>,
        Closure<dyn FnMut(ErrorEvent)>,
    )>,
    oauth2: Option<OAuth2Context>,
    _oauth2_handle: Option<ContextHandle<OAuth2Context>>,
    events: SharedTableModel<Entry>,

    application: String,

    running: bool,
    total_received: usize,
}

pub enum Msg {
    Start(Option<String>),
    StartPressed,
    Stop,
    Clear,
    Event(Box<Event>),
    /// Failed when processing an event
    Error(String),
    /// Source failed
    Failed,
    SetApplication(String),
    OAuth2Context(OAuth2Context),
}

const DEFAULT_MAX_SIZE: usize = 200;

impl Component for Spy {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let application = ctx.props().application.clone().unwrap_or_default();
        let (oauth2, oauth2_handle) = match ctx
            .link()
            .context::<OAuth2Context>(ctx.link().callback(|oauth| Msg::OAuth2Context(oauth)))
        {
            Some((oauth2, oauth2_handle)) => (Some(oauth2), Some(oauth2_handle)),
            _ => (None, None),
        };
        Self {
            events: Default::default(),
            ws: None,
            oauth2,
            _oauth2_handle: oauth2_handle,
            running: false,
            total_received: 0,
            application,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Start(app_id) => {
                log::info!("Starting: {:?}", app_id);
                self.start(ctx, app_id);
            }
            Msg::StartPressed => {
                ctx.link().send_message(Msg::Start(self.app_id_filter()));
            }
            Msg::Stop => {
                self.stop();
            }
            Msg::Clear => {
                self.total_received = 0;
                self.events.clear();
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
                error("Source error", "Connection to the websocket service failed");
                self.running = false;
            }
            Msg::SetApplication(application) => {
                self.application = application;
            }
            Msg::OAuth2Context(oauth2) => {
                // refresh the token of the connection
                if let (Some((ws, _, _)), Some(access_token)) = (&self.ws, oauth2.access_token()) {
                    match serde_json::to_string(&client::Message::RefreshAccessToken(
                        access_token.to_string(),
                    )) {
                        Ok(msg) => {
                            log::info!("Sending new access token: {}", msg);
                            let _ = ws.send_with_str(&msg);
                        }
                        Err(err) => {
                            log::error!("Failed to encode message: {err}");
                        }
                    }
                }
                // update our token
                self.oauth2 = Some(oauth2);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let is_valid = self.app_id_filter().is_some();
        let is_running = self.running;

        let header = html_nested! {
            <TableHeader>
                <TableColumn label="Timestamp (UTC)"/>
                <TableColumn label="Device ID"/>
                <TableColumn label="Channel"/>
                <TableColumn label="Payload"/>
            </TableHeader>
        };

        html! {
            <>
                <Toolbar>
                    <ToolbarGroup>

                        if ctx.props().application.is_none() {
                        <ToolbarItem>
                            <TextInput
                                disabled={self.running}
                                onchange={ctx.link().callback(Msg::SetApplication)}
                                validator={not_empty()}
                                placeholder="Application ID to spy on"/>
                        </ToolbarItem>
                        }

                        <ToolbarItem>
                            if is_running {
                                <Button
                                        label="Stop"
                                        icon={Icon::Pause}
                                        variant={Variant::Secondary}
                                        onclick={ctx.link().callback(|_|Msg::Stop)}
                                />
                            } else {
                                <Button
                                        disabled={!is_valid}
                                        label="Start"
                                        icon={Icon::Play}
                                        variant={Variant::Primary}
                                        onclick={ctx.link().callback(|_|Msg::StartPressed)}
                                />
                            }
                        </ToolbarItem>
                        <ToolbarItem>
                            <Button
                                label="Clear"
                                icon={Icon::Times}
                                variant={Variant::Secondary}
                                onclick={ctx.link().callback(|_|Msg::Clear)}
                                />
                        </ToolbarItem>
                    </ToolbarGroup>
                    <ToolbarItem modifiers={[ToolbarElementModifier::Right.all()]}>
                        if self.running {
                            <strong>{"Events received: "}{self.total_received}</strong>
                        }
                    </ToolbarItem>
                </Toolbar>

                <Table<SharedTableModel<Entry>>
                    entries={self.events.clone()}
                    mode={TableMode::CompactExpandable}
                    header={header}
                    >
                </Table<SharedTableModel<Entry>>>

                if self.events.is_empty() {
                    { self.render_empty() }
                }
            </>
        }
    }

    fn destroy(&mut self, _: &Context<Self>) {
        if let Some((ws, _, _)) = self.ws.take() {
            let _ = ws.close();
        }
    }
}

impl Spy {
    fn app_id_filter(&self) -> Option<String> {
        let value = self.application.clone();
        match value.is_empty() {
            true => None,
            false => Some(value),
        }
    }

    fn start(&mut self, ctx: &Context<Self>, app_id: Option<String>) {
        let ws_endpoint = &ctx.props().endpoints.endpoints.websocket_integration;

        let url = match (ws_endpoint, app_id) {
            (Some(ws), Some(app)) => {
                let mut url = Url::parse(ws.url.as_str()).unwrap();
                url.path_segments_mut().unwrap().push(app.as_str());
                Some(url)
            }
            _ => None,
        };

        if let (
            Some(mut url),
            Some(OAuth2Context::Authenticated(Authentication { access_token, .. })),
        ) = (url, self.oauth2.as_ref())
        {
            url.query_pairs_mut().append_pair("token", &access_token);

            let ws = WebSocket::new(url.as_str()).unwrap();

            // setup on_message callback
            let link = ctx.link().clone();
            let onmessage_callback = Closure::wrap(Box::new(move |event: &MessageEvent| {
                web_sys::console::debug_2(&wasm_bindgen::JsValue::from("event: "), event);

                let msg = match serde_json::from_str(&event.data().as_string().unwrap()) {
                    Ok(event) => Msg::Event(event),
                    Err(e) => Msg::Error(e.to_string()),
                };

                link.send_message(msg);
            }) as Box<dyn FnMut(&MessageEvent)>);

            // set message event handler on WebSocket
            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));

            // setup onerror
            let link = ctx.link().clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                log::warn!("error event: {:?}", e);
                link.send_message(Msg::Failed);
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            // store result
            self.running = true;
            self.ws = Some((ws, onmessage_callback, on_error));
        }
    }

    fn stop(&mut self) {
        if let Some((ws, _, _)) = self.ws.take() {
            let _ = ws.close();
        }
        self.running = false
    }

    fn render_empty(&self) -> Html {
        return html! {
            <div style="padding-bottom: 10rem; height: 100%;">
            <Bullseye>
            <EmptyState
                title="No new messages"
                icon={Icon::Pending}
                size={Size::XLarge}
                >
                { "The " } <q> {"message spy"} </q> { " will only show "} <strong> {"new"} </strong> {" messages received by the system.
                When the next message arrives, you will see it right here." }
            </EmptyState>
            </Bullseye>
            </div>
        };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry(pub Event);

impl TableRenderer for Entry {
    fn render(&self, col: ColumnIndex) -> Html {
        match col.index {
            // timestamp
            0 => render_timestamp(&self.0),
            // device id
            1 => self.device().into(),
            // channel
            2 => self.channel().into(),
            // payload
            3 => render_data_short(&self.0),
            // ignore
            _ => html! {},
        }
    }

    fn render_details(&self) -> Vec<Span> {
        vec![Span::max(render_details(&self.0)).truncate()]
    }
}

impl Entry {
    fn device(&self) -> String {
        self.extension_as_string(EXT_DEVICE)
    }

    fn channel(&self) -> String {
        self.0
            .subject()
            .map(ToString::to_string)
            .unwrap_or_default()
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

/// Render data for the details section
fn render_data(event: &Event) -> Html {
    // let data: Option<Data> = event.get_data();

    match event.data() {
        None => html! {},
        Some(Data::String(text)) => html! { <pre> {text} </pre> },
        Some(Data::Binary(blob)) => html! {
            <>
                <pre> { pretty_hex::pretty_hex(&blob) } </pre>
                <pre> { base64_block(blob) } </pre>
            </>
        },
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
                <Label label="String" color={Color::Purple}/>{" "}{truncate_str(100, text)}
            </pre>
        },
        Some(Data::Binary(blob)) => html! {
            <pre>
                <Label label="BLOB" color={Color::Blue}/>{" "}{render_blob(blob)}
            </pre>
        },
        Some(Data::Json(value)) => html! {
            <pre>
                <Label label="JSON" color={Color::Cyan}/>{" "}{truncate_str(100, &value.to_string())}
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
                    <pre class="pf-c-table__text">{ value.to_string() }</pre>
                },
            )
        })
        .map(|(key, value)| AttributeEntry(key, value))
        .collect();

    attrs.sort_by(|a, b| a.0.cmp(&b.0));

    let header = html_nested! (
        <TableHeader>
            <TableColumn label="Key"/>
            <TableColumn label="Value"/>
        </TableHeader>
    );

    let raw = serde_json::to_string_pretty(event)
        .map(|raw| html!(<pre> { raw } </pre>))
        .unwrap_or_else(|_| html!(<i>{"<Failed to encode event>"}</i>));

    html! (
        <>
            <h3>{"Attributes"}</h3>
            <Table<SharedTableModel<AttributeEntry>>
                entries={SharedTableModel::from(attrs)}
                mode={TableMode::CompactNoBorders}
                header={header}
                >
            </Table<SharedTableModel<AttributeEntry>>>

            <h3>{"Payload"}</h3>
            { render_data(event) }

            <h3>{"Raw"}</h3>
            { raw }
        </>
    )
}
