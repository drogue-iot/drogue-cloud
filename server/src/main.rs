use clap::{App, Arg, SubCommand};
use drogue_cloud_api_key_service::{endpoints as keys, service::KeycloakApiKeyService};
use drogue_cloud_device_management_service::service::PostgresManagementServiceConfig;
use drogue_cloud_endpoint_common::{auth::AuthConfig, command::KafkaCommandSourceConfig};
use drogue_cloud_registry_events::{sender::KafkaSenderConfig, stream::KafkaStreamConfig};
use drogue_cloud_service_api::{
    endpoints::*,
    kafka::{KafkaClientConfig, KafkaConfig},
};
use drogue_cloud_service_common::{
    client::RegistryConfig,
    client::UserAuthClientConfig,
    config::ConfigFromEnv,
    keycloak::KeycloakAdminClientConfig,
    openid::{AuthenticatorClientConfig, AuthenticatorConfig, AuthenticatorGlobalConfig},
};
use std::collections::HashMap;
use url::Url;

#[derive(Clone)]
struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Into<String> for Endpoint {
    fn into(self: Endpoint) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Clone)]
struct ServerConfig {
    pub api: Endpoint,
    pub console: Endpoint,
    pub mqtt: Endpoint,
    pub http: Endpoint,
    pub coap: Endpoint,
    pub mqtt_integration: Endpoint,
    pub websocket_integration: Endpoint,
    pub command: Endpoint,
    pub registry: Endpoint,
}

impl Default for ServerConfig {
    fn default() -> ServerConfig {
        ServerConfig {
            api: Endpoint {
                host: "localhost".to_string(),
                port: 10000,
            },
            console: Endpoint {
                host: "localhost".to_string(),
                port: 10001,
            },
            mqtt: Endpoint {
                host: "localhost".to_string(),
                port: 1883,
            },
            http: Endpoint {
                host: "localhost".to_string(),
                port: 8088,
            },
            coap: Endpoint {
                host: "localhost".to_string(),
                port: 5683,
            },
            mqtt_integration: Endpoint {
                host: "localhost".to_string(),
                port: 18883,
            },
            websocket_integration: Endpoint {
                host: "localhost".to_string(),
                port: 10002,
            },
            command: Endpoint {
                host: "localhost".to_string(),
                port: 10003,
            },
            registry: Endpoint {
                host: "localhost".to_string(),
                port: 10004,
            },
        }
    }
}

fn main() {
    env_logger::init();
    dotenv::dotenv().ok();
    let matches = App::new("Drogue Cloud Server")
        .about("Server for all Drogue Cloud components")
        .subcommand(
            SubCommand::with_name("run")
                .about("run server")
                .arg(
                    Arg::with_name("enable-all")
                        .long("--enable-all")
                        .help("enable all services"),
                )
                .arg(
                    Arg::with_name("enable-device-registry")
                        .long("--enable-device-registry")
                        .help("enable device management service"),
                )
                .arg(
                    Arg::with_name("enable-http-endpoint")
                        .long("--enable-http-endpoint")
                        .help("enable http endpoint"),
                )
                .arg(
                    Arg::with_name("enable-mqtt-endpoint")
                        .long("--enable-mqtt-endpoint")
                        .help("enable mqtt endpoint"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let server: ServerConfig = Default::default();

        let kafka_stream = |topic: &str, consumer_group: &str| KafkaStreamConfig {
            client: kafka_config(topic),
            consumer_group: consumer_group.to_string(),
        };

        let kafka_sender = |topic: &str| KafkaSenderConfig {
            client: kafka_config(topic),
            queue_timeout: None,
        };

        let command_source = |consumer_group: &str| KafkaCommandSourceConfig {
            topic: "iot-commands".to_string(),
            consumer_group: consumer_group.to_string(),
        };

        let oauth = AuthenticatorConfig {
            disabled: true,
            global: AuthenticatorGlobalConfig {
                sso_url: None,
                issuer_url: None,
                realm: "drogue".to_string(),
                redirect_url: None,
            },
            clients: HashMap::new(),
        };

        let keycloak = KeycloakAdminClientConfig {
            url: Url::parse(endpoints(&server).sso.as_ref().unwrap()).unwrap(),
            realm: "drogue".into(),
            admin_username: "admin".into(),
            admin_password: "admin123456".into(),
            tls_noverify: true,
        };

        let registry = RegistryConfig {
            url: Url::parse(&endpoints(&server).registry.as_ref().unwrap().url).unwrap(),
            token_config: None,
        };

        let mut pg = deadpool_postgres::Config::new();
        pg.dbname = Some("drogue".to_string());
        pg.host = Some("localhost:5432".to_string());
        pg.manager = Some(deadpool_postgres::ManagerConfig {
            recycling_method: deadpool_postgres::RecyclingMethod::Fast,
        });

        let database = PostgresManagementServiceConfig {
            pg: pg,
            instance: "drogue".to_string(),
        };

        let auth = AuthConfig {
            auth_disabled: true,
            url: Url::parse("http://localhost:8080").unwrap(),
            token_config: None,
        };

        let mut threads = Vec::new();
        if matches.is_present("enable-device-registry") || matches.is_present("enable-all") {
            log::info!("Enabling device registry");
            let o = oauth.clone();
            let k = keycloak.clone();
            let bind_addr = server.clone().registry.into();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_device_management_service::Config {
                    enable_api_keys: false,
                    user_auth: None,
                    oauth: o,
                    keycloak: k,
                    database_config: database.clone(),
                    health: None,
                    bind_addr,
                    kafka_sender: kafka_sender("registry"),
                };
                actix_rt::System::with_tokio_rt(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .worker_threads(1)
                        .thread_name("registry")
                        .build()
                        .unwrap()
                })
                .block_on(drogue_cloud_device_management_service::run(config))
                .unwrap();
            }));
        }

        if matches.is_present("enable-http-endpoint") || matches.is_present("enable-all") {
            log::info!("Enabling HTTP endpoint");
            let a = auth.clone();
            let command_source_kafka = command_source("http_endpoint");
            let bind_addr = server.http.into();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_http_endpoint::Config {
                    auth: a,
                    disable_tls: true,
                    health: None,
                    max_json_payload_size: 65536,
                    max_payload_size: 65536,
                    cert_bundle_file: None,
                    key_file: None,
                    command_source_kafka,
                    instance: "drogue".to_string(),
                    kafka_config: kafka_client(),
                    bind_addr,
                };
                actix_rt::System::with_tokio_rt(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .worker_threads(1)
                        .thread_name("http")
                        .enable_all()
                        .build()
                        .unwrap()
                })
                .block_on(drogue_cloud_http_endpoint::run(config))
                .unwrap();
            }));
        }

        /*
        if matches.is_present("enable-mqtt-endpoint") || matches.is_present("enable-all") {
            log::info!("Enabling MQTT endpoint");
            let a = auth.clone();
            let command_source_kafka = command_source("mqtt_endpoint");
            let bind_addr_mqtt = server.mqtt.into();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_mqtt_endpoint::Config {
                    auth: a,
                    health: None,
                    disable_tls: true,
                    cert_bundle_file: None,
                    key_file: None,
                    bind_addr_mqtt: Some(bind_addr_mqtt),
                    kafka_config: kafka_client(),
                    instance: "drogue".to_string(),
                    command_source_kafka,
                };

                let rt = ntex::rt::System::new("mqtt-endpoint");
                ntex::rt::System::set_current(rt);
                ntex::rt::System::current()
                    .block_on(drogue_cloud_mqtt_endpoint::run(config))
                    .unwrap();
            }));
        }
        */
        for t in threads.drain(..) {
            t.join().unwrap();
        }
        log::info!("All services stopped");
    }
}

const KAFKA_BOOTSTRAP: &'static str = "localhost:9092";

fn endpoints(config: &ServerConfig) -> Endpoints {
    Endpoints {
        api: Some(format!("http://{}:{}", config.api.host, config.api.port)),
        console: Some(format!(
            "http://{}:{}",
            config.console.host, config.console.port
        )),
        coap: Some(CoapEndpoint {
            url: format!("coap://{}:{}", config.coap.host, config.coap.port),
        }),
        http: Some(HttpEndpoint {
            url: format!("http://{}:{}", config.http.host, config.http.port),
        }),
        mqtt: Some(MqttEndpoint {
            host: config.mqtt.host.clone(),
            port: config.mqtt.port,
        }),
        mqtt_integration: Some(MqttEndpoint {
            host: config.mqtt_integration.host.clone(),
            port: config.mqtt_integration.port,
        }),
        websocket_integration: Some(HttpEndpoint {
            url: format!(
                "http://{}:{}",
                config.websocket_integration.host, config.websocket_integration.port
            ),
        }),
        sso: Some("http://localhost:8080".into()),
        issuer_url: Some("http://localhost:8080/auth".into()),
        redirect_url: Some("http://localhost:10000".into()),
        registry: Some(RegistryEndpoint {
            url: format!("http://{}:{}", config.registry.host, config.registry.port),
        }),
        command_url: Some(format!(
            "http://{}:{}",
            config.command.host, config.command.port
        )),
        local_certs: false,
        kafka_bootstrap_servers: Some(KAFKA_BOOTSTRAP.into()),
    }
}

fn kafka_client() -> KafkaClientConfig {
    KafkaClientConfig {
        bootstrap_servers: KAFKA_BOOTSTRAP.into(),
        properties: HashMap::new(),
    }
}

fn kafka_config(topic: &str) -> KafkaConfig {
    KafkaConfig {
        client: kafka_client(),
        topic: topic.to_string(),
    }
}
