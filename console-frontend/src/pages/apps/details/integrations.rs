use crate::{
    backend::{Backend, Token},
    utils::to_yaml_model,
};
use bstr::ByteVec;
use drogue_client::{
    registry::v1::{Application, KafkaAppStatus, KafkaUserStatus},
    Translator,
};
use drogue_cloud_service_api::{
    endpoints::Endpoints,
    kafka::{KafkaConfigExt, KafkaEventType, KafkaTarget},
};
use java_properties::PropertiesWriter;
use monaco::{api::CodeEditorOptions, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use serde_json::json;
use std::rc::Rc;
use yew::prelude::*;

pub struct IntegrationDetails<'a> {
    pub backend: &'a Backend,
    pub token: &'a Token,
    pub application: &'a Application,
    pub endpoints: &'a Endpoints,
}

impl IntegrationDetails<'_> {
    pub fn render(&self) -> Html {
        return html! {
            <Stack gutter=true>
                <StackItem>
                    { Self::wrap_card("Apache Kafka™", self.render_kafka()) }
                </StackItem>
                <StackItem>
                    { Self::wrap_card("MQTT", self.render_mqtt()) }
                </StackItem>
            </Stack>
        };
    }

    fn render_mqtt_quarkus(&self) -> Html {
        let mqtt = match &self.endpoints.mqtt_integration {
            Some(mqtt) => mqtt,
            _ => return html! {},
        };

        let user = self
            .token
            .userinfo
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or("<you>")
            .to_string();

        let options = CodeEditorOptions::default()
            .with_scroll_beyond_last_line(false)
            .with_language("yaml".to_owned())
            .with_builtin_theme(BuiltinTheme::VsDark);

        let options = Rc::new(options);

        let topic = format!("app/{}", self.application.metadata.name);

        let outgoing = json!({
            "type": "smallrye-mqtt",
            "ssl": true,
            "host": mqtt.host.clone(),
            "port": mqtt.port,
            "username": user,
            "password": "<api-key>",
        });

        let mut incoming = outgoing.clone();
        incoming["topic"] = topic.into();

        let content = json!({
            "mp": {
                "messaging": {
                    "incoming": {
                        "drogue-iot-incoming": incoming,
                    },
                    "outgoing": {
                        "drogue-iot-outgoing": outgoing,
                    },
                }
            },
        });

        if let Ok(model) = to_yaml_model(&content) {
            html! {
                <>
                <Tabs>
                    <Tab label="Quarkus">
                        <div><code>{"application.yaml"}</code></div>
                        <CodeEditor model=model options=options />
                    </Tab>
                </Tabs>
                </>
            }
        } else {
            html! {
                <Alert title="Failed to load editor" r#type=Type::Warning inline=true />
            }
        }
    }

    fn render_mqtt(&self) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[6]>{ self.render_mqtt_basic() }</GridItem>
                <GridItem cols=[6]>{ self.render_mqtt_quarkus() }</GridItem>
            </Grid>
        };
    }

    fn render_mqtt_basic(&self) -> Html {
        let mqtt = match &self.endpoints.mqtt_integration {
            Some(mqtt) => mqtt,
            _ => return html! {},
        };

        let user = self
            .token
            .userinfo
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or("<you>")
            .to_string();

        return html! {
            <DescriptionList>
                <DescriptionGroup term="Host">
                    <Clipboard readonly=true value=mqtt.host.clone() />
                </DescriptionGroup>
                <DescriptionGroup term="Port">
                    <Clipboard readonly=true value=mqtt.port.to_string() />
                </DescriptionGroup>
                <DescriptionGroup term="Version">
                    <strong>{ "v3.1.1" }</strong> {" or "} <strong> { "v5" } </strong>
                </DescriptionGroup>
                <DescriptionGroup term="TLS required">
                    <Switch checked=true disabled=true />
                </DescriptionGroup>
                <DescriptionGroup term="Credentials">
                    <Tabs>
                        <Tab label="OAuth2 Token">
                            <DescriptionList>
                                <DescriptionGroup term="Username (access token)">
                                    <Clipboard readonly=true value=self.token.access_token.clone() />
                                </DescriptionGroup>
                            </DescriptionList>
                        </Tab>
                        <Tab label="API key">
                            <DescriptionList>
                                <DescriptionGroup term="Username">
                                    <Clipboard readonly=true value=user />
                                </DescriptionGroup>
                                <DescriptionGroup term="Password (API key)">
                                    <TextInput readonly=true value="<api key>" />
                                </DescriptionGroup>
                            </DescriptionList>
                        </Tab>
                    </Tabs>
                </DescriptionGroup>
                <DescriptionGroup term="Device-to-Cloud subscription">
                    <Tabs>
                        <Tab label="Normal">
                            <Clipboard readonly=true value=format!("app/{}", self.application.metadata.name) />
                        </Tab>
                        <Tab label="Shared group">
                            <Clipboard readonly=true value=format!("$shared/<group>/app/{}", self.application.metadata.name) />
                        </Tab>
                    </Tabs>
                </DescriptionGroup>
                <DescriptionGroup term="Cloud-to-Device publishing">
                    <Clipboard readonly=true value=format!("command/{}/<device>/<command>", self.application.metadata.name) />
                </DescriptionGroup>
            </DescriptionList>
        };
    }

    fn render_kafka(&self) -> Html {
        let bootstrap = self
            .endpoints
            .kafka_bootstrap_servers
            .as_ref()
            .cloned()
            .unwrap_or_default();

        html! {
            <>
            <DescriptionList>
            {
                match self.application.kafka_target(KafkaEventType::Events) {
                    Ok(KafkaTarget::Internal { topic }) => {
                        html! {
                            <>
                            <DescriptionGroup term="Device-to-Cloud topic">
                                <Clipboard code=true readonly=true value=topic/>
                            </DescriptionGroup>
                            <DescriptionGroup term="Bootstrap Servers">
                                <Clipboard code=true readonly=true value=bootstrap/>
                            </DescriptionGroup>
                            </>
                        }
                    }
                    Ok(KafkaTarget::External { config }) => {
                        html! {
                             <DescriptionGroup term="Device-to-Cloud topic">
                                <Clipboard code=true readonly=true value=config.topic/>
                            </DescriptionGroup>
                        }
                    }
                    _ => html! {},
                }
            }

            {
                match self.application.section::<KafkaAppStatus>().and_then(|s| s.ok()).and_then(|s|s.user) {
                    Some(user) => html!{
                        <>
                        <DescriptionGroup term="User">
                            <Clipboard code=true readonly=true value=&user.username/>
                        </DescriptionGroup>
                        <DescriptionGroup term="Password">
                            <Clipboard code=true readonly=true value=&user.password/>
                        </DescriptionGroup>
                        <DescriptionGroup term="Mechanism">
                            <Clipboard code=true readonly=true value=&user.mechanism/>
                        </DescriptionGroup>
                        <DescriptionGroup term="JAAS Config">
                            <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable
                                value=Self::jaas_config(&user)/>
                        </DescriptionGroup>
                        <DescriptionGroup term="Consumer Properties">
                            <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable
                                value=Self::consumer_properties(&user)/>
                        </DescriptionGroup>
                        </>
                    },
                    None => html!{},
                }
            }
            </DescriptionList>
            </>
        }
    }

    fn wrap_card(title: &str, html: Html) -> Html {
        return html! {
            <Card
                title={html_nested!{<>{title}</>}}
                expandable=true large=true
                >
                { html }
            </Card>
        };
    }

    fn jaas_config(user: &KafkaUserStatus) -> String {
        format!(
            r#"org.apache.kafka.common.security.scram.ScramLoginModule required username="{}" password="{}";"#,
            user.username, user.password
        )
    }

    fn consumer_properties(user: &KafkaUserStatus) -> String {
        let mut properties: Vec<(&str, &str)> = Vec::new();

        let jaas = Self::jaas_config(user);

        properties.push(("security.protocol", "SASL_SSL"));
        properties.push(("sasl.mechanism", "SCRAM-SHA-512"));
        properties.push(("sasl.jaas.config", &jaas));

        Self::ser_properties(properties)
    }

    fn ser_properties(properties: Vec<(&str, &str)>) -> String {
        let mut buf = Vec::new();
        {
            let mut p = PropertiesWriter::new(&mut buf);
            for (k, v) in properties {
                p.write(k, v).ok();
            }
        }
        buf.into_string_lossy()
    }
}
