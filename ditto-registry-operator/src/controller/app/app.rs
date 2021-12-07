use super::{ConstructContext, DeconstructContext};
use crate::ditto::devops::{ConnectivityResponse, Headers, NamespaceResponse};
use crate::{
    controller::{policy_id, ControllerConfig},
    ditto::{
        self,
        devops::{
            Connection, ConnectionStatus, DevopsCommand, Enforcement, MappingDefinition,
            PiggybackCommand, QoS, Source,
        },
        Error,
    },
};
use async_trait::async_trait;
use drogue_client::{openid::AccessTokenProvider, registry::v1::Application};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{
    KafkaClientConfig, KafkaConfigExt, KafkaEventType, KafkaTarget,
};
use indexmap::IndexMap;
use serde_json::json;
use std::time::Duration;
use url::Url;

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

    async fn run(
        &self,
        ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let target = ctx.app.kafka_target(KafkaEventType::Events)?;
        let topic_name = target.topic_name().to_string();
        let connection_info = connection_info(&ctx.app, target, &self.config.kafka)?;
        let command = DevopsCommand {
            target_actor_selection: "/system/sharding/connection".to_string(),
            headers: Default::default(),
            piggyback_command: PiggybackCommand::ModifyConnection {
                connection: Connection {
                    id: connection_id(&ctx.app),
                    connection_type: "kafka".to_string(),
                    connection_status: ConnectionStatus::Open,
                    failover_enabled: true,
                    uri: connection_info.0,
                    specific_config: connection_info.1,
                    sources: vec![Source {
                        addresses: vec![topic_name],
                        qos: Some(QoS::AtLeastOnce),
                        consumer_count: 1,
                        authorization_context: vec!["pre-authenticated:drogue-cloud".to_string()],
                        enforcement: default_enforcement(&ctx.app),
                        header_mapping: default_header_mapping(),
                        payload_mapping: vec![
                            "drogue-cloud-events-mapping".to_string(),
                            "drogue-cloud-create-device".to_string(),
                        ],
                    }],
                    targets: vec![],
                    mapping_definitions: mapping_definitions(&ctx.app),
                },
            },
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

        // done

        Ok(OperationOutcome::Continue(ctx))
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
        let response: ConnectivityResponse = self
            .ditto
            .devops(
                self.provider,
                None,
                &DevopsCommand {
                    target_actor_selection: "/system/sharding/connection".into(),
                    headers: Default::default(),
                    piggyback_command: PiggybackCommand::DeleteConnection {
                        connection_id: connection_id(&ctx.app),
                    },
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
    let mechanism = config.client.properties.get("sasl.mechanisms");
    let bootstrap_server = config.client.bootstrap_servers;
    // fix for eclipse/ditto#1247
    let bootstrap_server = bootstrap_server.replace(".:", ":");

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

fn mapping_definitions(app: &Application) -> IndexMap<String, MappingDefinition> {
    let policy_id = policy_id(app);

    let mut map = IndexMap::with_capacity(2);
    map.insert(
        "drogue-cloud-events-mapping".to_string(),
        MappingDefinition {
            mapping_engine: "JavaScript".to_string(),
            options: {
                let mut options = IndexMap::new();

                options.insert(
                    "incomingScript".to_string(),
                    include_str!("../../../resources/ditto/incoming.js").into(),
                );
                options.insert(
                    "outgoingScript".to_string(),
                    include_str!("../../../resources/ditto/outgoing.js").into(),
                );
                options.insert("loadBytebufferJS".to_string(), "false".into());
                options.insert("loadLongJS".to_string(), "false".into());

                options
            },
        },
    );
    map.insert(
        "drogue-cloud-create-device".to_string(),
        MappingDefinition {
            mapping_engine: "ImplicitThingCreation".to_string(),
            options: {
                let mut options = IndexMap::new();
                options.insert(
                    "thing".to_string(),
                    json!({
                        "thingId": format!("{}:{{{{ header:ce_device }}}}", policy_id.0),
                        "policyId": policy_id.to_string(),
                    }),
                );
                options
            },
        },
    );
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

fn default_enforcement(app: &Application) -> Enforcement {
    let ns = &app.metadata.name;
    Enforcement {
        input: "{{ header:ce_application }}:{{ header:ce_device }}".to_string(),
        filters: vec![format!("{}:{{{{ thing:name }}}}", ns)],
        // filters: vec!["{{ entity:id }}".to_string()],
    }
}
