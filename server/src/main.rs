use clap::{App, Arg, SubCommand};
use diesel_migrations::embed_migrations;
use drogue_cloud_authentication_service::service::AuthenticationServiceConfig;
use drogue_cloud_device_management_service::service::PostgresManagementServiceConfig;
use drogue_cloud_endpoint_common::{auth::AuthConfig, command::KafkaCommandSourceConfig};
use drogue_cloud_registry_events::sender::KafkaSenderConfig; //, stream::KafkaStreamConfig};
use drogue_cloud_service_api::{
    endpoints::*,
    kafka::{KafkaClientConfig, KafkaConfig},
};
use drogue_cloud_service_common::{
    client::RegistryConfig,
    client::UserAuthClientConfig,
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig},
    openid::{AuthenticatorConfig, AuthenticatorGlobalConfig, TokenConfig},
};
use drogue_cloud_user_auth_service::service::AuthorizationServiceConfig;
use std::collections::HashMap;
use url::Url;

#[macro_use]
extern crate diesel_migrations;

embed_migrations!("../database-common/migrations");

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
    pub console: Endpoint,
    pub mqtt: Endpoint,
    pub http: Endpoint,
    pub coap: Endpoint,
    pub mqtt_integration: Endpoint,
    pub websocket_integration: Endpoint,
    pub command: Endpoint,
    pub registry: Endpoint,
    pub device_auth: Endpoint,
    pub user_auth: Endpoint,
}

impl Default for ServerConfig {
    fn default() -> ServerConfig {
        ServerConfig {
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
            device_auth: Endpoint {
                host: "localhost".to_string(),
                port: 10005,
            },
            user_auth: Endpoint {
                host: "localhost".to_string(),
                port: 10006,
            },
        }
    }
}

fn run_migrations() {
    use diesel::Connection;
    log::info!("Migrating database");
    let database_url = "postgres://admin:admin123456@localhost:5432/drogue";
    let connection = diesel::PgConnection::establish(database_url)
        .expect(&format!("Error connecting to {}", database_url));

    embedded_migrations::run_with_output(&connection, &mut std::io::stdout()).unwrap();
    log::info!("Migration done");
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
                    Arg::with_name("enable-console-backend")
                        .long("--enable-console-backend")
                        .help("enable console backend service"),
                )
                .arg(
                    Arg::with_name("enable-device-registry")
                        .long("--enable-device-registry")
                        .help("enable device management service"),
                )
                .arg(
                    Arg::with_name("enable-user-authentication-service")
                        .long("--enable-user-authentication-service")
                        .help("enable user authentication service"),
                )
                .arg(
                    Arg::with_name("enable-authentication-service")
                        .long("--enable-authentication-service")
                        .help("enable device authentication service"),
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
        run_migrations();

        let server: ServerConfig = Default::default();

        /*
        let kafka_stream = |topic: &str, consumer_group: &str| KafkaStreamConfig {
            client: kafka_config(topic),
            consumer_group: consumer_group.to_string(),
        };
        */

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
                sso_url: endpoints(&server).sso,
                issuer_url: endpoints(&server).issuer_url,
                realm: "master".to_string(),
                redirect_url: None,
            },
            clients: HashMap::new(),
        };

        let keycloak = KeycloakAdminClientConfig {
            url: Url::parse(endpoints(&server).sso.as_ref().unwrap()).unwrap(),
            realm: "master".into(),
            admin_username: "admin".into(),
            admin_password: "admin123456".into(),
            tls_noverify: true,
        };

        let registry = RegistryConfig {
            url: Url::parse(&endpoints(&server).registry.as_ref().unwrap().url).unwrap(),
            token_config: None,
        };

        let mut pg = deadpool_postgres::Config::new();
        pg.host = Some("localhost".to_string());
        pg.port = Some(5432);
        pg.user = Some("admin".to_string());
        pg.password = Some("admin123456".to_string());
        pg.dbname = Some("drogue".to_string());
        pg.manager = Some(deadpool_postgres::ManagerConfig {
            recycling_method: deadpool_postgres::RecyclingMethod::Fast,
        });

        let authurl: String = server.device_auth.clone().into();
        let auth = AuthConfig {
            auth_disabled: true,
            url: Url::parse(&format!("http://{}", authurl)).unwrap(),
            token_config: None,
        };

        let client_secret = "myimportantsecret";
        let token_config = TokenConfig {
            client_id: "drogue-service".to_string(),
            client_secret: client_secret.to_string(),
            issuer_url: endpoints(&server)
                .issuer_url
                .as_ref()
                .map(|u| Url::parse(u).unwrap()),
            sso_url: Url::parse(endpoints(&server).sso.as_ref().unwrap()).ok(),
            realm: "master".to_string(),
            refresh_before: None,
        };

        let mut threads = Vec::new();
        if matches.is_present("enable-device-registry") || matches.is_present("enable-all") {
            log::info!("Enabling device registry");
            let o = oauth.clone();
            let k = keycloak.clone();
            let bind_addr = server.clone().registry.into();
            let s = server.clone();
            let pg = pg.clone();
            let t = token_config.clone();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_device_management_service::Config {
                    enable_api_keys: false,
                    user_auth: Some(UserAuthClientConfig {
                        token_config: Some(t),
                        url: Url::parse(&format!(
                            "http://{}:{}",
                            s.user_auth.host, s.user_auth.port
                        ))
                        .unwrap(),
                    }),
                    oauth: o,
                    keycloak: k,
                    database_config: PostgresManagementServiceConfig {
                        pg: pg.clone(),
                        instance: "drogue".to_string(),
                    },

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

        if matches.is_present("enable-user-authentication-service")
            || matches.is_present("enable-all")
        {
            log::info!("Enabling user authentication service");
            let o = oauth.clone();
            let k = keycloak.clone();
            let bind_addr = server.clone().user_auth.into();
            let pg = pg.clone();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_user_auth_service::Config {
                    max_json_payload_size: 65536,
                    oauth: o,
                    keycloak: k,
                    health: None,
                    service: AuthorizationServiceConfig { pg },
                    bind_addr,
                };
                actix_rt::System::with_tokio_rt(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .worker_threads(1)
                        .thread_name("authentication-service")
                        .build()
                        .unwrap()
                })
                .block_on(drogue_cloud_user_auth_service::run::<KeycloakAdminClient>(
                    config,
                ))
                .unwrap();
            }));
        }

        if matches.is_present("enable-authentication-service") || matches.is_present("enable-all") {
            log::info!("Enabling device authentication service");
            let o = oauth.clone();
            let bind_addr = server.clone().device_auth.into();
            let pg = pg.clone();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_authentication_service::Config {
                    max_json_payload_size: 65536,
                    oauth: o,
                    health: None,
                    auth_service_config: AuthenticationServiceConfig { pg },
                    bind_addr,
                };
                actix_rt::System::with_tokio_rt(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .worker_threads(1)
                        .thread_name("authentication-service")
                        .build()
                        .unwrap()
                })
                .block_on(drogue_cloud_authentication_service::run(config))
                .unwrap();
            }));
        }

        if matches.is_present("enable-console-backend") || matches.is_present("enable-all") {
            log::info!("Enabling console backend service");
            let o = oauth.clone();
            let s = server.clone();
            let t = token_config.clone();
            let bind_addr = s.console.clone().into();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_console_backend::Config {
                    oauth: o,
                    health: None,
                    bind_addr,
                    enable_kube: false,
                    kafka: kafka_client(),
                    keycloak,
                    registry,
                    console_token_config: Some(t.clone()),
                    disable_account_url: false,
                    scopes: "openid profile email".into(),
                    user_auth: Some(UserAuthClientConfig {
                        token_config: Some(t),
                        url: Url::parse(&format!(
                            "http://{}:{}",
                            s.user_auth.host, s.user_auth.port
                        ))
                        .unwrap(),
                    }),
                };
                actix_rt::System::with_tokio_rt(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .worker_threads(1)
                        .thread_name("console-backend-service")
                        .build()
                        .unwrap()
                })
                .block_on(drogue_cloud_console_backend::run(config, endpoints(&s)))
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
                    kafka_downstream_config: kafka_client(),
                    kafka_command_config: kafka_client(),
                    check_kafka_topic_ready: false,
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
        api: None,
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
        issuer_url: Some("http://localhost:8080/auth/realms/master".into()),
        redirect_url: Some("http://localhost:10001".into()),
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
