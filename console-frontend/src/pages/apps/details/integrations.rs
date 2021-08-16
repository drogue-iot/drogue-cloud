use crate::{
    backend::{Backend, Token},
    utils::{shell_single_quote, to_model, to_yaml_model},
};
use bstr::ByteVec;
use drogue_client::{
    registry::v1::{Application, KafkaAppStatus, KafkaUserStatus},
    Translator,
};
use drogue_cloud_service_api::{
    endpoints::{Endpoints, HttpEndpoint, MqttEndpoint},
    kafka::{KafkaConfigExt, KafkaEventType, KafkaTarget},
};
use java_properties::PropertiesWriter;
use monaco::{
    api::{CodeEditorOptions, TextModel},
    sys::editor::BuiltinTheme,
    yew::CodeEditor,
};
use patternfly_yew::*;
use serde_json::json;
use std::rc::Rc;
use wasm_bindgen::JsValue;
use yew::prelude::*;

struct KafkaInfo {
    pub bootstrap: String,
    pub target: KafkaTarget,
    pub user: KafkaUserStatus,
}

pub struct IntegrationDetails<'a> {
    pub backend: &'a Backend,
    pub token: &'a Token,
    pub application: &'a Application,
    pub endpoints: &'a Endpoints,
}

impl IntegrationDetails<'_> {
    pub fn render(&self) -> Html {
        let mut items: Vec<Html> = Vec::new();

        if let Some(kafka) = self.kafka_info() {
            items.push(Self::wrap_card("Apache Kafka™", self.render_kafka(&kafka)));
        }

        if let Some(mqtt) = &self.endpoints.mqtt_integration {
            items.push(Self::wrap_card("MQTT", self.render_mqtt(mqtt)));
        }

        if let Some(ws) = &self.endpoints.websocket_integration {
            items.push(Self::wrap_card("Websocket", self.render_ws(&ws)));
        }

        return html! {
            <Stack gutter=true>
                {
                    for items.into_iter().map(|item|html!{<StackItem> {item} </StackItem>})
                }
            </Stack>
        };
    }

    fn render_mqtt_quarkus(&self, mqtt: &MqttEndpoint) -> Html {
        let user = self
            .token
            .userinfo
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or("<you>")
            .to_string();

        let topic = format!("app/{}", self.application.metadata.name);

        let outgoing = json!({
            "connector": "smallrye-mqtt",
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

        return html! {
            <Tabs>
                <Tab label="Quarkus">
                    <div><code>{"application.yaml"}</code></div>
                    { Self::default_editor(Some("yaml"), to_yaml_model(&content)) }
                </Tab>
            </Tabs>
        };
    }

    fn default_editor(language: Option<&str>, model: Result<TextModel, JsValue>) -> Html {
        let mut options = CodeEditorOptions::default()
            .with_scroll_beyond_last_line(false)
            .with_builtin_theme(BuiltinTheme::VsDark);

        if let Some(language) = language {
            options = options.with_language(language.to_owned());
        }

        let options = Rc::new(options);

        if let Ok(model) = model {
            html! {
                <CodeEditor model=model options=options />
            }
        } else {
            html! {
                <Alert title="Failed to load editor" r#type=Type::Warning inline=true />
            }
        }
    }

    fn render_mqtt(&self, mqtt: &MqttEndpoint) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[6]>{ self.render_mqtt_basic(mqtt) }</GridItem>
                <GridItem cols=[6]>{ self.render_mqtt_quarkus(mqtt) }</GridItem>
            </Grid>
        };
    }

    fn render_mqtt_basic(&self, mqtt: &MqttEndpoint) -> Html {
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

    fn kafka_info(&self) -> Option<KafkaInfo> {
        let bootstrap = self.endpoints.kafka_bootstrap_servers.as_ref().cloned();

        let target = self.application.kafka_target(KafkaEventType::Events);

        let user = self
            .application
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok())
            .and_then(|s| s.user);

        match (bootstrap, target, user) {
            (Some(bootstrap), Ok(target), Some(user)) => Some(KafkaInfo {
                bootstrap,
                target,
                user,
            }),
            _ => None,
        }
    }

    fn render_kafka(&self, info: &KafkaInfo) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[6]>{ self.render_kafka_basic(&info) }</GridItem>
                <GridItem cols=[6]>{ self.render_kafka_examples(&info) }</GridItem>
            </Grid>
        };
    }

    fn render_kafka_examples(&self, info: &KafkaInfo) -> Html {
        let topic = match &info.target {
            KafkaTarget::Internal { topic } => topic.clone(),
            KafkaTarget::External { config } => config.topic.clone(),
        };

        let podman = {
            let mut command = format!(
                r#"podman run --rm -ti docker.io/bitnami/kafka:latest kafka-console-consumer.sh
--topic {topic}
--bootstrap-server {bootstrap}"#,
                topic = topic,
                bootstrap = info.bootstrap,
            );

            for (k, v) in Self::consumer_properties(&info.user) {
                command += &format!("\n--consumer-property {}={}", k, shell_single_quote(v));
            }

            command.replace('\n', " \\\n\t")
        };

        let quarkus = {
            let mut props = Self::consumer_properties(&info.user);
            props.insert(0, ("connector".into(), "smallrye-kafka".into()));
            props.insert(0, ("topic".into(), topic));
            let mut props = Self::ser_properties(
                props
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            format!("mp.messaging.incoming.drogue-iot-incoming.{}", k),
                            v,
                        )
                    })
                    .collect::<Vec<_>>(),
            );

            props += r#"
# or use "latest" to start with the most recent event
mp.messaging.incoming.drogue-iot-incoming.auto.offset.reset=earliest"#;

            props
        };

        let knative = {
            let yaml = format!(
                r#"---
apiVersion: sources.knative.dev/v1beta1
kind: KafkaSource
metadata:
  name: drogue-iot-source
spec:
  bootstrapServers:
    - {server}
  
  # consumerGroup: replace with your own Kafka consumer group
  consumerGroup: my-group
  
  # consumer: increase if you need more than one pod consuming events
  consumers: 1
  
  net:
    sasl:
      enable: true
      type:
        secretKeyRef:
          name: drogue-iot-source-secret
          key: mechanism
      user:
        secretKeyRef:
          name: drogue-iot-source-secret
          key: user
      password:
        secretKeyRef:
          name: drogue-iot-source-secret
          key: password
    tls:
      caCert: {{}}
      cert: {{}}
      key: {{}}
  sink:
    # Define a reference to a Service receiving the CloudEvents.
    # This is a service that you have to provide.
    # Also see: https://knative.dev/docs/developer/eventing/sinks/ 
    ref:
      apiVersion: v1
      kind: Service
      name: my-service
---
apiVersion: v1
kind: Secret
metadata:
  name: drogue-iot-source-secret
stringData:
  mechanism: {mechanism}
  user: {user}
  password: {password}
"#,
                server = &info.bootstrap,
                mechanism = &info.user.mechanism,
                user = &info.user.username,
                password = &info.user.password,
            );

            yaml
        };

        return html! {
                <>
                <Tabs>
                    <Tab label="Command line">
                        <Clipboard
                            code=true readonly=true variant=ClipboardVariant::Expanded
                            value=podman/>
                    </Tab>
                    <Tab label="Quarkus">
                        <div><code>{"application.properties"}</code></div>
                        { Self::default_editor(Some("properties"), to_model(Some("properties"), &quarkus)) }
                    </Tab>
                    <Tab label="Knative">
                        <div><code>{"kafka-source.yaml"}</code></div>
                        { Self::default_editor(Some("yaml"), to_model(Some("yaml"), &knative)) }
                    </Tab>
                </Tabs>
                </>
        };
    }

    fn render_kafka_basic(&self, info: &KafkaInfo) -> Html {
        html! {
            <>
            <DescriptionList>
                {
                    match &info.target {
                        KafkaTarget::Internal { topic } => {
                            html! {
                                <>
                                <DescriptionGroup term="Device-to-Cloud topic">
                                    <Clipboard code=true readonly=true value=topic/>
                                </DescriptionGroup>
                                <DescriptionGroup term="Bootstrap Servers">
                                    <Clipboard code=true readonly=true value=&info.bootstrap/>
                                </DescriptionGroup>
                                </>
                            }
                        }
                        KafkaTarget::External { config } => {
                            html! {
                                 <DescriptionGroup term="Device-to-Cloud topic">
                                    <Clipboard code=true readonly=true value=&config.topic/>
                                </DescriptionGroup>
                            }
                        }
                    }
                }

                <DescriptionGroup term="User">
                    <Clipboard code=true readonly=true value=&info.user.username/>
                </DescriptionGroup>
                <DescriptionGroup term="Password">
                    <Clipboard code=true readonly=true value=&info.user.password/>
                </DescriptionGroup>
                <DescriptionGroup term="Mechanism">
                    <Clipboard code=true readonly=true value=&info.user.mechanism/>
                </DescriptionGroup>
                <DescriptionGroup term="JAAS Config">
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable
                        value=Self::jaas_config(&info.user)/>
                </DescriptionGroup>
                <DescriptionGroup term="Consumer Properties">
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable
                        value=Self::consumer_properties_str(&info.user)/>
                </DescriptionGroup>
            </DescriptionList>
            </>
        }
    }

    fn render_ws(&self, ws: &HttpEndpoint) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[6]>{ self.render_ws_basic(ws) }</GridItem>
                <GridItem cols=[6]>{ self.render_ws_example(ws) }</GridItem>
            </Grid>
        };
    }

    fn render_ws_basic(&self, ws: &HttpEndpoint) -> Html {
        let user = self
            .token
            .userinfo
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or("<you>")
            .to_string();

        return html! {
            <DescriptionList>
                <DescriptionGroup term="Url">
                    <Clipboard readonly=true value=ws.url.clone() />
                </DescriptionGroup>
                <DescriptionGroup term="TLS required">
                    <Switch checked=true disabled=true />
                </DescriptionGroup>
                <DescriptionGroup term="Credentials">
                    <Tabs>
                        <Tab label="OAuth2 Token">
                            <DescriptionList>
                                <DescriptionGroup term="Authentication token)">
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
                <DescriptionGroup term="Application events subscription">
                    <Clipboard readonly=true value=self.application.metadata.name.clone() />
                </DescriptionGroup>
            </DescriptionList>
        };
    }

    fn render_ws_example(&self, ws: &HttpEndpoint) -> Html {
        let websocat = format!(
            r#"websocat {}/{} -H="Authorization: Bearer {}""#,
            ws.url.clone(),
            self.application.metadata.name,
            self.token.access_token.clone(),
        );

        return html! {
                <>
                <Tabs>
                    <Tab label="Command line">
                        <Clipboard
                            code=true readonly=true variant=ClipboardVariant::Expanded
                            value=websocat/>
                    </Tab>
                </Tabs>
                </>
        };
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

    fn consumer_properties(user: &KafkaUserStatus) -> Vec<(String, String)> {
        let mut properties: Vec<(&str, &str)> = Vec::new();

        let jaas = Self::jaas_config(user);

        properties.push(("security.protocol", "SASL_SSL"));
        properties.push(("sasl.mechanism", "SCRAM-SHA-512"));
        properties.push(("sasl.jaas.config", &jaas));

        properties
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    fn consumer_properties_str(user: &KafkaUserStatus) -> String {
        Self::ser_properties(Self::consumer_properties(user))
    }

    fn ser_properties(properties: Vec<(String, String)>) -> String {
        let mut buf = Vec::new();
        {
            let mut p = PropertiesWriter::new(&mut buf);
            for (k, v) in properties {
                p.write(&k, &v).ok();
            }
        }
        buf.into_string_lossy()
    }
}
