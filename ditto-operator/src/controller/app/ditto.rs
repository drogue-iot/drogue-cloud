use super::{ConstructContext, DeconstructContext};
use crate::{
    controller::ControllerConfig,
    ditto::{
        self, Connection, ConnectionStatus, DevopsCommand, Enforcement, Error, MappingDefinition,
        PiggybackCommand, Source,
    },
};
use async_trait::async_trait;
use drogue_client::registry::v1::Application;
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{
    KafkaClientConfig, KafkaConfigExt, KafkaEventType, KafkaTarget,
};
use indexmap::IndexMap;
use url::Url;

pub struct CreateApplication<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateApplication<'o> {
    fn type_name(&self) -> String {
        "CreateApplication".into()
    }

    async fn run(
        &self,
        ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let target = ctx.app.kafka_target(KafkaEventType::Events)?;
        let topic_name = target.topic_name().to_string();
        let connection_info = connection_info(&ctx.app, target, &self.config.kafka)?;
        self.ditto
            .devops(DevopsCommand {
                target_actor_selection: "/system/sharding/connection".to_string(),
                headers: Default::default(),
                piggyback_command: PiggybackCommand::CreateConnection {
                    connection: Connection {
                        id: connection_id(&ctx.app),
                        connection_type: "kafka".to_string(),
                        connection_status: ConnectionStatus::Open,
                        failover_enabled: true,
                        uri: connection_info.0,
                        specific_config: connection_info.1,
                        sources: vec![Source {
                            addresses: vec![topic_name],
                            consumer_count: 1,
                            authorization_context: vec![
                                "pre-authenticated:drogue-cloud".to_string()
                            ],
                            enforcement: default_enforcement(),
                            header_mapping: default_header_mapping(),
                            payload_mapping: vec!["drogue-cloud-events-mapping".to_string()],
                        }],
                        targets: vec![],
                        mapping_definitions: mapping_definitions(),
                    },
                },
            })
            .await
            .map_err(map_ditto_error)?;
        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct DeleteApplication<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
}

impl<'o> DeleteApplication<'o> {
    pub async fn run(&self, ctx: &mut DeconstructContext) -> Result<(), ReconcileError> {
        // done

        self.ditto
            .devops(DevopsCommand {
                target_actor_selection: "/system/sharding/connection".into(),
                headers: Default::default(),
                piggyback_command: PiggybackCommand::DeleteConnection {
                    connection_id: connection_id(&ctx.app),
                },
            })
            .await
            .map_err(map_ditto_error)?;

        Ok(())
    }
}

fn map_ditto_error(err: Error) -> ReconcileError {
    match err {
        ditto::Error::Request(err)
            if err.is_timeout()
                || err.is_connect()
                || err
                    .status()
                    .map(|code| code.is_server_error())
                    .unwrap_or_default() =>
        {
            ReconcileError::temporary(err)
        }
        err => ReconcileError::permanent(err),
    }
}

fn connection_id(app: &Application) -> String {
    format!("kafka-drogue-{}", app.metadata.name)
}

fn connection_info(
    app: &Application,
    target: KafkaTarget,
    default_config: &KafkaClientConfig,
) -> Result<(String, IndexMap<String, String>), ReconcileError> {
    // evaluate the full kafka config

    let config = target.into_config(default_config);

    // extract what we need

    let username = config.client.properties.get("sasl.username");
    let password = config
        .client
        .properties
        .get("sasl.password")
        .map(|s| s.as_str());
    let mechanism = config.client.properties.get("sasl.mechanism");
    let bootstrap_server = config.client.bootstrap_servers;

    // assemble all information

    let mut url = Url::parse(&format!("tcp://{}", &bootstrap_server)).map_err(|err| {
        log::info!("Failed to build Kafka bootstrap URL: {}", err);
        ReconcileError::permanent("Failed to build Kafka bootstrap URL")
    })?;
    if let Some(username) = username {
        url.set_username(username)
            .and_then(|_| url.set_password(password))
            .map_err(|_| {
                log::info!("Failed to set username or password on Kafka bootstrap URL");
                ReconcileError::permanent("Failed to build Kafka bootstrap URL")
            })?;
    }
    let url = url.to_string();

    let mut map = IndexMap::new();
    map.insert("bootstrapServers".to_string(), bootstrap_server);
    if let Some(mechanism) = mechanism {
        map.insert("saslMechanism".to_string(), mechanism.to_string());
    }
    map.insert("groupId".to_string(), connection_id(app));

    // return

    Ok((url, map))
}

fn mapping_definitions() -> IndexMap<String, MappingDefinition> {
    let mut options = IndexMap::new();

    options.insert(
        "incomingScript".to_string(),
        include_str!("../../../resources/ditto/incoming.js").to_string(),
    );
    options.insert(
        "outgoingScript".to_string(),
        include_str!("../../../resources/ditto/outgoing.js").to_string(),
    );
    options.insert("loadBytebufferJS".to_string(), "false".to_string());
    options.insert("loadLongJS".to_string(), "false".to_string());

    let def = MappingDefinition {
        mapping_engine: "JavaScript".to_string(),
        options,
    };

    let mut map = IndexMap::with_capacity(1);
    map.insert("drogue-cloud-events-mapping".to_string(), def);
    map
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

fn default_enforcement() -> Enforcement {
    Enforcement {
        input: "{{ header:ce_application }}:{{ header:ce_device }}".to_string(),
        filters: vec!["{{ entity:id }}".to_string()],
    }
}
