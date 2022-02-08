use super::{ConstructContext, DeconstructContext};
use crate::data::{ExporterTarget, Ingress};
use crate::{
    controller::ControllerConfig,
    data::{DittoAppSpec, DittoTopic, Exporter, ExporterMode},
    ditto::{
        self,
        devops::{
            Connection, ConnectionStatus, ConnectivityResponse, DevopsCommand, Enforcement,
            Headers, MappingDefinition, NamespaceResponse, PiggybackCommand, QoS, Source, Target,
        },
        Error,
    },
};
use async_trait::async_trait;
use drogue_client::{openid::AccessTokenProvider, registry::v1::Application, Translator};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{self, OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{
    KafkaClientConfig, KafkaConfigExt, KafkaEventType, KafkaTarget,
};
use indexmap::IndexMap;
use std::time::Duration;
use url::Url;

struct DittoKafkaOptions {
    pub uri: String,
    pub specific_config: IndexMap<String, String>,
    pub validate_certificates: bool,
    pub ca: Option<String>,
}

pub struct CreateApplication<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o Option<AccessTokenProvider>,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateApplication<'o> {
    fn type_name(&self) -> String {
        "CreateApplication".into()
    }

    async fn run(&self, ctx: ConstructContext) -> progress::Result<ConstructContext> {
        let spec = ctx
            .app
            .section::<DittoAppSpec>()
            .transpose()
            .map_err(|err| {
                ReconcileError::permanent(format!("Failed to parse Ditto spec: {}", err))
            })?;

        // create inbound connection

        let ctx = self.create_inbound(ctx, spec.as_ref()).await?;

        // create (optional) outbound connection

        let ctx = if let Some(exporter) = spec.and_then(|spec| spec.exporter) {
            self.create_outbound(ctx, exporter).await?
        } else {
            delete_connection(
                self.ditto,
                self.provider,
                ConnectionType::Outbound.connection_id(&ctx.app),
            )
            .await?;
            ctx
        };

        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}

impl<'a> CreateApplication<'a> {
    async fn create_inbound(
        &self,
        ctx: ConstructContext,
        spec: Option<&DittoAppSpec>,
    ) -> Result<ConstructContext, ReconcileError> {
        self.create_connection(inbound_connection_definition(
            &ctx,
            spec.and_then(|spec| spec.ingress.as_ref()),
            &self.config.kafka,
        )?)
        .await?;

        Ok(ctx)
    }

    async fn create_outbound(
        &self,
        ctx: ConstructContext,
        exporter: Exporter,
    ) -> Result<ConstructContext, ReconcileError> {
        self.create_connection(outbound_connection_definition(&ctx, exporter)?)
            .await?;

        Ok(ctx)
    }

    async fn create_connection(&self, connection: Connection) -> Result<(), ReconcileError> {
        let command = DevopsCommand {
            target_actor_selection: "/system/sharding/connection".to_string(),
            headers: Default::default(),
            piggyback_command: PiggybackCommand::ModifyConnection { connection },
        };

        // execute and get response

        let mut response: ConnectivityResponse = self
            .ditto
            .devops(self.provider, None, &command)
            .await
            .map_err(map_ditto_error)?;

        // check if we need to create it first

        if let Some(r) = response.connectivity.values().next() {
            if r.status == 404 {
                log::debug!("Connection doesn't exist yet, creating one.");
                let connection = match command.piggyback_command {
                    PiggybackCommand::ModifyConnection { connection } => connection,
                    _ => return Err(ReconcileError::permanent("This is weird!")),
                };

                // connection doesn't exist yet, let's create it
                response = self
                    .ditto
                    .devops(
                        self.provider,
                        None,
                        &DevopsCommand {
                            target_actor_selection: "/system/sharding/connection".to_string(),
                            headers: Default::default(),
                            piggyback_command: PiggybackCommand::CreateConnection { connection },
                        },
                    )
                    .await
                    .map_err(map_ditto_error)?;
            }
        }

        eval_con_response(response)?;

        Ok(())
    }
}

pub struct DeleteApplication<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o Option<AccessTokenProvider>,
}

impl<'o> DeleteApplication<'o> {
    pub async fn run(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        self.delete_connection(ctx).await?;
        self.block_namespace(ctx).await?;
        self.purge_namespace(ctx).await?;
        self.unblock_namespace(ctx).await?;
        Ok(())
    }

    async fn delete_connection(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        delete_connection(
            self.ditto,
            self.provider,
            ConnectionType::Inbound.connection_id(&ctx.app),
        )
        .await?;
        delete_connection(
            self.ditto,
            self.provider,
            ConnectionType::Outbound.connection_id(&ctx.app),
        )
        .await?;
        Ok(())
    }

    async fn block_namespace(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        let response = self
            .ditto
            .devops(
                self.provider,
                None,
                &DevopsCommand {
                    target_actor_selection: "/system/distributedPubSubMediator".into(),
                    headers: Default::default(),
                    piggyback_command: PiggybackCommand::BlockNamespace {
                        namespace: ctx.app.metadata.name.clone(),
                    },
                },
            )
            .await
            .map_err(map_ditto_error)?;

        eval_ns_response(response)
    }

    async fn unblock_namespace(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        let response = self
            .ditto
            .devops(
                self.provider,
                None,
                &DevopsCommand {
                    target_actor_selection: "/system/distributedPubSubMediator".into(),
                    headers: Default::default(),
                    piggyback_command: PiggybackCommand::UnblockNamespace {
                        namespace: ctx.app.metadata.name.clone(),
                    },
                },
            )
            .await
            .map_err(map_ditto_error)?;

        eval_ns_response(response)
    }

    async fn purge_namespace(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        let response = self
            .ditto
            .devops(
                self.provider,
                Some(Duration::from_secs(20)),
                &DevopsCommand {
                    target_actor_selection: "/system/distributedPubSubMediator".into(),
                    headers: Headers {
                        aggregate: true,
                        is_group_topic: true,
                    },
                    piggyback_command: PiggybackCommand::PurgeNamespace {
                        namespace: ctx.app.metadata.name.clone(),
                    },
                },
            )
            .await
            .map_err(map_ditto_error)?;

        eval_ns_response(response)
    }
}

fn inbound_connection_definition(
    ctx: &ConstructContext,
    ingress: Option<&Ingress>,
    default_config: &KafkaClientConfig,
) -> Result<Connection, ReconcileError> {
    let target = ctx.app.kafka_target(KafkaEventType::Events)?;
    let topic_name = target.topic_name().to_string();
    let group_id = ConnectionType::Inbound.connection_id(&ctx.app);
    let DittoKafkaOptions {
        uri,
        specific_config,
        validate_certificates,
        ca,
    } = connection_info_from_target(target, group_id, default_config)?;
    Ok(Connection {
        id: ConnectionType::Inbound.connection_id(&ctx.app),
        connection_type: "kafka".to_string(),
        connection_status: ConnectionStatus::Open,
        client_count: ingress.as_ref().and_then(|i| i.clients),
        failover_enabled: true,
        uri,
        specific_config,
        validate_certificates,
        ca,
        sources: vec![Source {
            addresses: vec![topic_name],
            qos: Some(QoS::AtLeastOnce),
            consumer_count: ingress.as_ref().and_then(|i| i.consumers).unwrap_or(1),
            authorization_context: vec!["pre-authenticated:drogue-cloud".to_string()],
            enforcement: default_enforcement(&ctx.app),
            header_mapping: default_header_mapping(),
            payload_mapping: vec!["drogue-cloud-events-mapping".to_string()],
        }],
        targets: vec![],
        mapping_definitions: mapping_definitions_inbound(),
    })
}

fn outbound_connection_target(
    ctx: &ConstructContext,
    exporter: ExporterTarget,
) -> Result<Target, ReconcileError> {
    if exporter.topic.contains('/') {
        return Err(ReconcileError::permanent(format!(
            "Ditto exporter Kafka topic must not contain slashes. Is: {}",
            exporter.topic
        )));
    }

    let namespace = ctx.app.metadata.name.clone();

    let topics = exporter
        .subscriptions
        .into_iter()
        .map(|topic| {
            let mut query = IndexMap::<String, String>::new();
            query.insert("namespace".to_string(), namespace.clone());

            Result::<_, ReconcileError>::Ok(match topic {
                DittoTopic::TwinEvents {
                    extra_fields,
                    filter,
                } => {
                    if !extra_fields.is_empty() {
                        query.insert("extraFields".to_string(), extra_fields.join(","));
                    }
                    if let Some(filter) = filter {
                        query.insert("filter".to_string(), filter);
                    }
                    format!(
                        "_/_/things/twin/events?{}",
                        serde_urlencoded::to_string(query).map_err(|err| {
                            ReconcileError::permanent(format!(
                                "Unable to encode outbound configuration options: {}",
                                err
                            ))
                        })?
                    )
                }
            })
        })
        .collect::<Result<_, _>>()?;

    let (payload_mapping, header_mapping) = mapping_definitions_outbound(&exporter.mode);

    Ok(Target {
        address: format!("{}/{{{{ thing:id }}}}", exporter.topic),
        authorization_context: vec!["pre-authenticated:drogue-cloud".to_string()],
        header_mapping,
        payload_mapping,
        topics,
    })
}

fn outbound_connection_definition(
    ctx: &ConstructContext,
    exporter: Exporter,
) -> Result<Connection, ReconcileError> {
    let targets = exporter
        .targets
        .into_iter()
        .map(|target| outbound_connection_target(ctx, target))
        .collect::<Result<_, _>>()?;

    let DittoKafkaOptions {
        uri,
        specific_config,
        validate_certificates,
        ca,
    } = connection_info(
        &exporter.kafka,
        ConnectionType::Outbound.connection_id(&ctx.app),
    )?;

    let connection = Connection {
        id: ConnectionType::Outbound.connection_id(&ctx.app),
        connection_type: "kafka".to_string(),
        connection_status: ConnectionStatus::Open,
        failover_enabled: true,
        client_count: None,
        uri,
        specific_config,
        validate_certificates,
        ca,
        targets,
        sources: vec![],
        mapping_definitions: Default::default(),
    };

    Ok(connection)
}

async fn delete_connection(
    ditto: &ditto::Client,
    provider: &Option<AccessTokenProvider>,
    connection_id: String,
) -> Result<(), ReconcileError> {
    let response: ConnectivityResponse = ditto
        .devops(
            provider,
            None,
            &DevopsCommand {
                target_actor_selection: "/system/sharding/connection".into(),
                headers: Default::default(),
                piggyback_command: PiggybackCommand::DeleteConnection { connection_id },
            },
        )
        .await
        .map_err(map_ditto_error)?;

    if let Some(r) = response.connectivity.values().next() {
        if r.status == 404 {
            log::debug!("Connection was already gone");
            return Ok(());
        }
    }

    eval_con_response(response)
}

fn eval_con_response(mut response: ConnectivityResponse) -> Result<(), ReconcileError> {
    let r = response.connectivity.pop();

    if let Some((_, r)) = r {
        match r.status {
            200..=299 => Ok(()),
            code => Err(ReconcileError::permanent(format!(
                "DevOps error ({}): {}",
                code,
                r.error.unwrap_or_default(),
            ))),
        }
    } else {
        Err(ReconcileError::permanent("Missing devops response"))
    }
}

fn eval_ns_response(mut response: NamespaceResponse) -> Result<(), ReconcileError> {
    let r = response.entries.pop();

    if let Some((_, r)) = r {
        match r.status {
            200..=299 => Ok(()),
            code => Err(ReconcileError::permanent(format!(
                "DevOps error ({})",
                code,
            ))),
        }
    } else {
        Err(ReconcileError::permanent("Missing devops response"))
    }
}

fn map_ditto_error(err: Error) -> ReconcileError {
    log::info!("Ditto devops error: {:?}", err);
    match err.is_temporary() {
        true => ReconcileError::temporary(err),
        false => ReconcileError::permanent(err),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Inbound,
    Outbound,
}

impl ConnectionType {
    pub fn connection_id(&self, app: &Application) -> String {
        match self {
            Self::Inbound => format!("kafka-drogue-{}", app.metadata.name),
            Self::Outbound => format!("drogue-out-{}", app.metadata.name),
        }
    }
}

fn connection_info_from_target(
    target: KafkaTarget,
    group_id: String,
    default_config: &KafkaClientConfig,
) -> Result<DittoKafkaOptions, ReconcileError> {
    let config = target.into_config(default_config);
    connection_info(&config, group_id)
}

fn connection_info(
    config: &KafkaClientConfig,
    group_id: String,
) -> Result<DittoKafkaOptions, ReconcileError> {
    // extract what we need

    let username = config.properties.get("sasl.username");
    let password = config.properties.get("sasl.password").map(|s| s.as_str());
    let mechanism = config.properties.get("sasl.mechanism");
    let bootstrap_server = &config.bootstrap_servers;
    // fix for eclipse/ditto#1247
    let bootstrap_server = bootstrap_server.replace(".:", ":");

    let scheme = match config
        .properties
        .get("security.protocol")
        .map(|s| s.as_str())
    {
        Some("SSL") | Some("SASL_SSL") => "ssl",
        _ => "tcp",
    };

    // assemble all information

    let mut uri = Url::parse(&format!("{}://{}", scheme, &bootstrap_server)).map_err(|err| {
        log::info!("Failed to build Kafka bootstrap URL: {}", err);
        ReconcileError::permanent("Failed to build Kafka bootstrap URL")
    })?;
    if let Some(username) = username {
        uri.set_username(username)
            .and_then(|_| uri.set_password(password))
            .map_err(|_| {
                log::info!("Failed to set username or password on Kafka bootstrap URL");
                ReconcileError::permanent("Failed to build Kafka bootstrap URL")
            })?;
    }
    let uri = uri.to_string();

    let mut specific_config = IndexMap::new();
    specific_config.insert("bootstrapServers".to_string(), bootstrap_server);
    if let Some(mechanism) = mechanism {
        specific_config.insert("saslMechanism".to_string(), mechanism.to_string());
    }
    specific_config.insert("groupId".to_string(), group_id);

    // extract some ditto specific values

    let validate_certificates = config
        .properties
        .get("ditto.validateCertificates")
        .map(|s| s == "true")
        .unwrap_or(true);

    let ca = config.properties.get("ditto.ca").cloned();

    // copy over all "ditto.specificConfig" prefixed properties to the specific config

    for (k, v) in &config.properties {
        if let Some(k) = k.strip_prefix("ditto.specificConfig.") {
            specific_config.insert(k.to_string(), v.to_string());
        }
    }

    // return

    Ok(DittoKafkaOptions {
        uri,
        specific_config,
        validate_certificates,
        ca,
    })
}

fn mapping_definitions_inbound() -> IndexMap<String, MappingDefinition> {
    let mut map = IndexMap::with_capacity(2);
    map.insert(
        "drogue-cloud-events-mapping".to_string(),
        MappingDefinition {
            mapping_engine: "JavaScript".to_string(),
            options: {
                let mut options = IndexMap::new();

                options.insert(
                    "incomingScript".to_string(),
                    include_str!("../../../resources/ditto/incoming/to_ditto.js").into(),
                );
                options.insert(
                    "outgoingScript".to_string(),
                    include_str!("../../../resources/ditto/incoming/from_ditto.js").into(),
                );
                options.insert("loadBytebufferJS".to_string(), "false".into());
                options.insert("loadLongJS".to_string(), "false".into());

                options
            },
        },
    );
    map
}

fn mapping_definitions_outbound(mode: &ExporterMode) -> (Vec<String>, IndexMap<String, String>) {
    match mode {
        ExporterMode::Ditto { normalized } => (
            match normalized {
                true => vec!["Normalized".to_string()],
                false => vec![],
            },
            Default::default(),
        ),
        ExporterMode::CloudEvents { normalized } => {
            let mappers = match normalized {
                true => vec!["Normalized".to_string()],
                false => vec![],
            };

            let mut headers = IndexMap::new();
            headers.insert("ce_specversion".to_string(), "1.0".to_string());
            headers.insert(
                "ce_source".to_string(),
                "ditto:instance/{{ thing:namespace }}/{{ thing:name }}".to_string(),
            );
            headers.insert(
                "ce_id".to_string(),
                "{{ header:correlation-id }}".to_string(),
            );
            headers.insert("ce_time".to_string(), "{{ time:now }}".to_string());
            headers.insert(
                "ce_type".to_string(),
                "org.eclipse.io.ditto.v{{ header:version }}".to_string(),
            );
            headers.insert("ce_dataschema".to_string(), "urn:eclipse:ditto".to_string());
            headers.insert(
                "ce_application".to_string(),
                "{{ thing:namespace }}".to_string(),
            );
            headers.insert("ce_device".to_string(), "{{ thing:name }}".to_string());
            headers.insert("ce_subject".to_string(), "{{ topic:action }}".to_string());

            (mappers, headers)
        }
    }
}

fn default_header_mapping() -> IndexMap<String, String> {
    let mut map = IndexMap::new();
    map.insert(
        "application".to_string(),
        "{{ header:ce_application }}".to_string(),
    );
    map.insert("device".to_string(), "{{ header:ce_device }}".to_string());
    map.insert(
        "content-type".to_string(),
        "{{ header:content-type }}".to_string(),
    );
    map
}

fn default_enforcement(app: &Application) -> Enforcement {
    let ns = &app.metadata.name;
    Enforcement {
        input: "{{ header:ce_application }}:{{ header:ce_device }}".to_string(),
        filters: vec![format!("{}:{{{{ thing:name }}}}", ns)],
        // filters: vec!["{{ entity:id }}".to_string()],
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_client::registry;
    use serde_json::json;

    #[test]
    fn test_spec() {
        let json = json!({
            "metadata":{
                "name": "app1",
            },
            "spec": {
                "ditto":{
                    "exporter":{
                        "kafka":{
                        },
                        "targets": [
                            {
                                "topic": "kafka-topic",
                                "mode": {
                                    "ditto": {},
                                },
                                "subscriptions": [
                                    {
                                        "twinEvents": {},
                                    },
                                    {
                                        "twinEvents": {
                                            "extraFields": [
                                                "attributes/fooBar",
                                                "features/light",
                                            ]
                                        },
                                    },
                                    {
                                        "twinEvents": {
                                            "extraFields": [
                                                "attributes/placement",
                                                "foo,bar,baz",
                                            ],
                                            "filter": r#"gt(attributes/placement,"Kitchen")"#,
                                        },
                                    }
                                ]
                            }
                        ],
                    },
                },
            }
        });

        let app: registry::v1::Application = serde_json::from_value(json).unwrap();
        let spec: DittoAppSpec = app.section().unwrap().unwrap();

        let connection =
            outbound_connection_definition(&ConstructContext { app }, spec.exporter.unwrap())
                .unwrap();

        assert_eq!(connection.connection_status, ConnectionStatus::Open);
        assert_eq!(connection.connection_type, "kafka");
        assert_eq!(connection.sources, vec![]);
        assert_eq!(
            connection.targets,
            vec![Target {
                address: "kafka-topic/{{ thing:id }}".to_string(),
                topics: vec![
                    "_/_/things/twin/events?namespace=app1".to_string(),
                    "_/_/things/twin/events?namespace=app1&extraFields=attributes%2FfooBar%2Cfeatures%2Flight".to_string(),
                    "_/_/things/twin/events?namespace=app1&extraFields=attributes%2Fplacement%2Cfoo%2Cbar%2Cbaz&filter=gt%28attributes%2Fplacement%2C%22Kitchen%22%29".to_string()
                ],
                authorization_context: vec!["pre-authenticated:drogue-cloud".to_string()],
                header_mapping: {
                    let m = IndexMap::new();
                    m
                },
                payload_mapping: vec![],
            }]
        )
    }
}
