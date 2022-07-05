mod config;
mod db;
mod keycloak;

use crate::{config::*, keycloak::*};
use anyhow::anyhow;
use clap::{crate_version, App, Arg, SubCommand};
use drogue_cloud_authentication_service::service::AuthenticationServiceConfig;
use drogue_cloud_database_common::postgres;
use drogue_cloud_device_management_service::service::PostgresManagementServiceConfig;
use drogue_cloud_device_state_service::service::postgres::PostgresServiceConfiguration;
use drogue_cloud_endpoint_common::{auth::AuthConfig, command::KafkaCommandSourceConfig};
use drogue_cloud_mqtt_common::server::{MqttServerOptions, Transport};
use drogue_cloud_registry_events::sender::KafkaSenderConfig; //, stream::KafkaStreamConfig};
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_api::webapp::HttpServer;
use drogue_cloud_service_common::actix::{CorsBuilder, HttpBuilder};
use drogue_cloud_service_common::{
    actix::HttpConfig,
    client::{DeviceStateClientConfig, RegistryConfig, UserAuthClientConfig},
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig},
    openid::{
        AuthenticatorClientConfig, AuthenticatorConfig, AuthenticatorGlobalConfig, TokenConfig,
    },
    state::StateControllerConfiguration,
};
use drogue_cloud_user_auth_service::service::AuthorizationServiceConfig;
use futures::TryFutureExt;
use std::{collections::HashMap, future::Future, pin::Pin, time::Duration};
use tokio::runtime::Handle;
use url::Url;

#[macro_use]
extern crate diesel_migrations;

fn args() -> App<'static, 'static> {
    App::new("Drogue Cloud Server")
        .about("Running Drogue Cloud in a single process")
        .version(crate_version!())
        .long_about("Drogue Server runs all the Drogue Cloud services in a single process, with an external dependency on PostgreSQL, Kafka and Keycloak for storing data, device management and user management")
        .arg(
            Arg::with_name("verbose")
                .global(true)
                .long("verbose")
                .short("v")
                .multiple(true)
                .help("Be verbose. Can be used multiple times to increase verbosity.")
        )
        .arg(
            Arg::with_name("quiet")
                .global(true)
                .long("quiet")
                .short("q")
                .conflicts_with("verbose")
                .help("Be quiet.")
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("run server")
                .arg(
                    Arg::with_name("insecure")
                        .long("--insecure")
                        .help("Run insecure, like disabling TLS checks")
                )
                .arg(
                    Arg::with_name("bind-address")
                        .long("--bind-address")
                        .help("bind to specific network address (default localhost)")
                        .value_name("ADDRESS")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("enable-all")
                        .long("--enable-all")
                        .help("enable all services"),
                )
                .arg(
                    Arg::with_name("enable-api")
                        .long("--enable-api")
                        .help("enable API backend service"),
                )
                .arg(
                    Arg::with_name("enable-console-frontend")
                        .long("--enable-console-frontend")
                        .help("enable console frontend serving"),
                )
                .arg(
                    // helps when enable-all is active, but you don't want the console
                    Arg::with_name("disable-console-frontend")
                        .long("--disable-console-frontend")
                        .conflicts_with("enable-console-frontend")
                        .help("disable console frontend serving"),
                )
                .arg(
                    Arg::with_name("enable-device-registry")
                        .long("--enable-device-registry")
                        .help("enable device management service"),
                )
                .arg(
                    Arg::with_name("enable-device-state")
                        .long("--enable-device-state")
                        .help("enable device state service"),
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
                    Arg::with_name("enable-coap-endpoint")
                        .long("--enable-coap-endpoint")
                        .help("enable coap endpoint"),
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
                )
                .arg(
                    Arg::with_name("enable-mqtt-integration")
                        .long("--enable-mqtt-integration")
                        .help("enable mqtt integration"),
                )
                .arg(
                    Arg::with_name("enable-websocket-integration")
                        .long("--enable-websocket-integration")
                        .help("enable websocket integration"),
                )
                .arg(
                    Arg::with_name("enable-command-endpoint")
                        .long("--enable-command-endpoint")
                        .help("enable command endpoint"),
                )
                .arg(
                    Arg::with_name("server-key")
                        .long("--server-key")
                        .value_name("FILE")
                        .help("private key to use for service endpoints")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("server-cert")
                        .long("--server-cert")
                        .value_name("FILE")
                        .help("public certificate to use for service endpoints")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("database-host")
                        .long("--database-host")
                        .value_name("HOST")
                        .help("hostname of PostgreSQL database")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("database-port")
                        .long("--database-port")
                        .value_name("PORT")
                        .help("port of PostgreSQL database")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("database-name")
                        .long("--database-name")
                        .value_name("NAME")
                        .help("name of database to use")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("database-user")
                        .long("--database-user")
                        .value_name("USER")
                        .help("username to use with database")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("database-password")
                        .long("--database-password")
                        .value_name("PASSWORD")
                        .help("password to use with database")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("keycloak-url")
                        .long("--keycloak-url")
                        .value_name("URL")
                        .help("url for Keycloak")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("keycloak-realm")
                        .long("--keycloak-realm")
                        .value_name("REALM")
                        .help("Keycloak realm to use")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("keycloak-user")
                        .long("--keycloak-user")
                        .value_name("USER")
                        .help("Keycloak realm admin user")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("keycloak-password")
                        .long("--keycloak-password")
                        .value_name("PASSWORD")
                        .help("Keycloak realm admin password")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("kafka-bootstrap-servers")
                        .long("--kafka-bootstrap-servers")
                        .value_name("HOSTS")
                        .help("Kafka bootstrap servers")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("ui-dist")
                        .long("--ui-dist")
                        .value_name("PATH")
                        .env("UI_DIST")
                        .help("Path to the UI distribution bundle")
                        .takes_value(true)
                ),
        )
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let mut app = args();
    let matches = app.clone().get_matches();

    stderrlog::new()
        .verbosity((matches.occurrences_of("verbose") + 1) as usize)
        .quiet(matches.is_present("quiet"))
        .init()
        .unwrap();

    if let Some(matches) = matches.subcommand_matches("run") {
        let tls = matches.is_present("server-cert") && matches.is_present("server-key");
        let server: ServerConfig = ServerConfig::new(matches);
        let eps = endpoints(&server, tls);

        db::run_migrations(&server.database).await.unwrap();

        configure_keycloak(&server).await.unwrap();
        /*
        let kafka_stream = |topic: &str, consumer_group: &str| KafkaStreamConfig {
            client: kafka_config(topic),
            consumer_group: consumer_group.to_string(),
        };
        */
        let http_prefix = if tls { "https" } else { "http" };
        let mqtt_prefix = if tls { "mqtts" } else { "mqtt" };
        let ws_prefix = if tls { "wss" } else { "ws" };

        let kafka_sender = |topic: &str, config: &KafkaClientConfig| KafkaSenderConfig {
            client: kafka_config(config, topic),
            queue_timeout: None,
        };

        let command_source = |consumer_group: &str| KafkaCommandSourceConfig {
            topic: "iot-commands".to_string(),
            consumer_group: consumer_group.to_string(),
        };

        let token_config = TokenConfig {
            client_id: "services".to_string(),
            client_secret: SERVICE_CLIENT_SECRET.to_string(),
            issuer_url: eps.issuer_url.as_ref().map(|u| Url::parse(u).unwrap()),
            sso_url: Url::parse(eps.sso.as_ref().unwrap()).ok(),
            realm: server.keycloak.realm.clone(),
            refresh_before: None,
            tls_insecure: server.tls_insecure,
            tls_ca_certificates: server.tls_ca_certificates.clone().into(),
        };

        let mut oauth = AuthenticatorConfig {
            disabled: false,
            global: AuthenticatorGlobalConfig {
                sso_url: eps.sso.clone(),
                issuer_url: eps.issuer_url.clone(),
                realm: server.keycloak.realm.clone(),
                redirect_url: eps.redirect_url.clone(),
                tls_insecure: server.tls_insecure,
                tls_ca_certificates: server.tls_ca_certificates.clone().into(),
            },
            clients: HashMap::new(),
        };
        oauth.clients.insert(
            "drogue".to_string(),
            AuthenticatorClientConfig {
                client_id: "drogue".to_string(),
                client_secret: SERVICE_CLIENT_SECRET.to_string(),
                scopes: "openid profile email".into(),
                issuer_url: None,
                tls_insecure: Some(server.tls_insecure),
                tls_ca_certificates: Some(server.tls_ca_certificates.clone().into()),
            },
        );
        oauth.clients.insert(
            "services".to_string(),
            AuthenticatorClientConfig {
                client_id: "services".to_string(),
                client_secret: SERVICE_CLIENT_SECRET.to_string(),
                scopes: "openid profile email".into(),
                issuer_url: None,
                tls_insecure: Some(server.tls_insecure),
                tls_ca_certificates: Some(server.tls_ca_certificates.clone().into()),
            },
        );

        let keycloak = KeycloakAdminClientConfig {
            url: Url::parse(eps.sso.as_ref().unwrap()).unwrap(),
            realm: server.keycloak.realm.clone(),
            admin_username: server.keycloak.user.clone(),
            admin_password: server.keycloak.password.clone(),
            tls_insecure: server.tls_insecure,
            tls_ca_certificates: server.tls_ca_certificates.clone().into(),
        };

        let registry = RegistryConfig {
            url: Url::parse(&eps.registry.as_ref().unwrap().url).unwrap(),
            token_config: Some(token_config.clone()),
        };

        let mut db = deadpool_postgres::Config::new();
        db.host = Some(server.database.endpoint.host.clone());
        db.port = Some(server.database.endpoint.port);
        db.user = Some(server.database.user.clone());
        db.password = Some(server.database.password.clone());
        db.dbname = Some(server.database.db.clone());
        db.manager = Some(deadpool_postgres::ManagerConfig {
            recycling_method: deadpool_postgres::RecyclingMethod::Fast,
        });
        let pg = postgres::Config {
            db,
            tls: Default::default(),
        };

        let authurl: String = server.device_auth.clone().into();
        let auth = AuthConfig {
            auth_disabled: false,
            url: Url::parse(&format!("http://{}", authurl)).unwrap(),
            client: Default::default(),
            token_config: Some(token_config.clone()),
        };

        let user_auth = Some(UserAuthClientConfig {
            token_config: Some(token_config.clone()),
            url: Url::parse(&format!(
                "http://{}:{}",
                server.user_auth.host, server.user_auth.port
            ))
            .unwrap(),
        });

        let state = StateControllerConfiguration {
            client: DeviceStateClientConfig {
                url: Url::parse(&format!(
                    "http://{}:{}",
                    server.device_state.host, server.device_state.port
                ))
                .unwrap(),
                token_config: Some(token_config.clone()),
                ..Default::default()
            },
            init_delay: Some(Duration::from_secs(2)),
            ..Default::default()
        };

        let mut tasks: Vec<Pin<Box<dyn Future<Output = anyhow::Result<()>>>>> = Vec::new();

        let oauth = oauth.clone();
        let server = server.clone();
        let auth = auth.clone();
        let registry = registry.clone();
        let matches = matches.clone();
        let user_auth = user_auth.clone();

        if matches.is_present("enable-device-state") || matches.is_present("enable-all") {
            log::info!("Enabling device state service");

            let kafka = server.kafka.clone();
            let config = drogue_cloud_device_state_service::Config {
                http: HttpConfig {
                    bind_addr: server.device_state.clone().into(),
                    disable_tls: true,
                    workers: Some(1),
                    metrics_namespace: Some("device_state_service".into()),
                    ..Default::default()
                },

                enable_access_token: true,
                oauth: oauth.clone(),
                service: PostgresServiceConfiguration {
                    session_timeout: Duration::from_secs(10),
                    pg: pg.clone(),
                },
                instance: "drogue".to_string(),
                check_kafka_topic_ready: false,
                kafka_downstream_config: kafka,
                health: None,
                endpoint_pool: Default::default(),
                registry: registry.clone(),
            };

            tasks.push(Box::pin(drogue_cloud_device_state_service::run(config)));
        }

        if matches.is_present("enable-user-authentication-service")
            || matches.is_present("enable-all")
        {
            log::info!("Enabling user authentication service");
            let config = drogue_cloud_user_auth_service::Config {
                http: HttpConfig {
                    bind_addr: server.user_auth.clone().into(),
                    disable_tls: true,
                    workers: Some(1),
                    metrics_namespace: Some("user_authentication_service".into()),
                    ..Default::default()
                },
                oauth: oauth.clone(),
                keycloak: keycloak.clone(),
                health: None,
                service: AuthorizationServiceConfig { pg: pg.clone() },
            };

            tasks.push(Box::pin(drogue_cloud_user_auth_service::run::<
                KeycloakAdminClient,
            >(config)));
        }

        if matches.is_present("enable-authentication-service") || matches.is_present("enable-all") {
            log::info!("Enabling device authentication service");
            let config = drogue_cloud_authentication_service::Config {
                http: HttpConfig {
                    bind_addr: server.device_auth.clone().into(),
                    disable_tls: true,
                    workers: Some(1),
                    metrics_namespace: Some("authentication_service".into()),
                    ..Default::default()
                },
                oauth: oauth.clone(),
                health: None,
                auth_service_config: AuthenticationServiceConfig { pg: pg.clone() },
            };

            tasks.push(Box::pin(drogue_cloud_authentication_service::run(config)));
        }

        if matches.is_present("enable-api") || matches.is_present("enable-all") {
            log::info!("Enabling composite API service");
            let console_config = {
                let mut console_token_config = token_config.clone();
                console_token_config.client_id = "drogue".to_string();
                drogue_cloud_console_backend::Config {
                    http: Default::default(), // overridden later on
                    oauth: oauth.clone(),
                    health: None,
                    enable_kube: false,
                    kafka: server.kafka.clone(),
                    keycloak: keycloak.clone(),
                    registry: registry.clone(),
                    console_token_config: Some(console_token_config),
                    disable_account_url: false,
                    scopes: "openid profile email".into(),
                    user_auth: user_auth.clone(),
                    enable_access_token: true,
                }
            };

            let config_device_management_service = drogue_cloud_device_management_service::Config {
                http: Default::default(), // overridden later on
                enable_access_token: true,
                user_auth: user_auth.clone(),
                oauth: oauth.clone(),
                keycloak: keycloak.clone(),
                database_config: PostgresManagementServiceConfig {
                    pg: pg.clone(),
                    instance: server.database.db.to_string(),
                },
                health: None,
                kafka_sender: kafka_sender("registry", &server.kafka.clone()),
            };

            let config_command = {
                let kafka = server.kafka.clone();
                let user_auth = user_auth.clone();

                drogue_cloud_command_endpoint::Config {
                    http: Default::default(), // overridden later on
                    health: None,
                    enable_access_token: true,
                    oauth: oauth.clone(),
                    registry: registry.clone(),
                    instance: "drogue".to_string(),
                    check_kafka_topic_ready: false,
                    command_kafka_sink: kafka,
                    user_auth,
                    endpoint_pool: Default::default(),
                }
            };

            let http = HttpConfig {
                bind_addr: server.console.clone().into(),
                disable_tls: true,
                workers: Some(1),
                metrics_namespace: Some("console_backend".into()),
                ..Default::default()
            };

            let (console_backend, _) =
                drogue_cloud_console_backend::configurator(console_config, endpoints(&server, tls))
                    .await
                    .unwrap();

            let (registry, _) = drogue_cloud_device_management_service::configurator(
                config_device_management_service,
            )
            .await
            .unwrap();

            let (command, _) = drogue_cloud_command_endpoint::configurator(config_command)
                .await
                .unwrap();

            tasks.push(Box::pin(async move {
                HttpBuilder::new(http, move |cfg| {
                    console_backend(cfg);
                    registry(cfg);
                    command(cfg);
                })
                .cors(CorsBuilder::Permissive)
                .run()?
                .await
            }));
        }

        if matches.is_present("enable-http-endpoint") || matches.is_present("enable-all") {
            log::info!("Enabling HTTP endpoint");
            let command_source_kafka = command_source("http_endpoint");
            let kafka = server.kafka.clone();
            let cert_bundle_file: Option<String> =
                matches.value_of("server-cert").map(|s| s.to_string());
            let key_file: Option<String> = matches.value_of("server-key").map(|s| s.to_string());

            let config = drogue_cloud_http_endpoint::Config {
                http: HttpConfig {
                    workers: Some(1),
                    disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                    cert_bundle_file,
                    key_file,
                    bind_addr: server.http.clone().into(),
                    metrics_namespace: Some("http_endpoint".into()),
                    ..Default::default()
                },
                auth: auth.clone(),
                health: None,
                command_source_kafka,
                instance: "drogue".to_string(),
                kafka_downstream_config: kafka.clone(),
                kafka_command_config: kafka,
                check_kafka_topic_ready: false,
                endpoint_pool: Default::default(),
            };

            tasks.push(Box::pin(drogue_cloud_http_endpoint::run(config)));
        }

        if matches.is_present("enable-websocket-integration") || matches.is_present("enable-all") {
            log::info!("Enabling Websocket integration");
            let bind_addr = server.websocket_integration.clone().into();
            let cert_bundle_file: Option<String> =
                matches.value_of("server-cert").map(|s| s.to_string());
            let key_file: Option<String> = matches.value_of("server-key").map(|s| s.to_string());
            let kafka = server.kafka.clone();
            let user_auth = user_auth.clone();
            let config = drogue_cloud_websocket_integration::Config {
                http: HttpConfig {
                    disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                    workers: Some(1),
                    bind_addr,
                    cert_bundle_file,
                    key_file,
                    metrics_namespace: Some("websocket_integration".into()),
                    ..Default::default()
                },
                health: None,
                enable_access_token: true,
                oauth: oauth.clone(),

                registry: registry.clone(),
                kafka,
                user_auth,
            };

            // The websocket integration uses the actix actors, so for now, that must run
            // on an actix runtime.
            tasks.push(Box::pin(async {
                Handle::current()
                    .spawn_blocking(move || {
                        let runner = actix_rt::System::with_tokio_rt(|| {
                            tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .worker_threads(1)
                                .max_blocking_threads(1)
                                .thread_name("actix")
                                .build()
                                .unwrap()
                        });

                        runner.block_on(async move {
                            drogue_cloud_websocket_integration::run(config).await
                        })
                    })
                    .await??;

                Ok::<(), anyhow::Error>(())
            }));
        }

        // ntex related tasks
        {
            let oauth = oauth.clone();
            let server = server.clone();
            let auth = auth.clone();
            let matches = matches.clone();

            let command_source_kafka = command_source("mqtt_endpoint");
            let bind_addr_mqtt = server.mqtt.clone().into();
            let bind_addr_mqtt_ws = server.mqtt_ws.clone().into();
            let kafka = server.kafka.clone();
            let cert_bundle_file: Option<String> =
                matches.value_of("server-cert").map(|s| s.to_string());
            let key_file: Option<String> = matches.value_of("server-key").map(|s| s.to_string());

            let mut mqtt_endpoints: Vec<drogue_cloud_mqtt_endpoint::Config> = vec![];
            let mut mqtt_integrations: Vec<drogue_cloud_mqtt_integration::Config> = vec![];

            if matches.is_present("enable-mqtt-endpoint") || matches.is_present("enable-all") {
                log::info!("Enabling MQTT endpoint");

                let config = drogue_cloud_mqtt_endpoint::Config {
                    mqtt: MqttServerOptions {
                        workers: Some(1),
                        bind_addr: Some(bind_addr_mqtt),
                        ..Default::default()
                    },
                    endpoint: Default::default(),
                    auth,
                    health: None,
                    disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                    disable_client_certificates: false,
                    cert_bundle_file,
                    key_file,
                    instance: "drogue".to_string(),
                    command_source_kafka,
                    kafka_downstream_config: kafka.clone(),
                    kafka_command_config: kafka,
                    check_kafka_topic_ready: false,
                    endpoint_pool: Default::default(),
                    state: state.clone(),
                };

                //tasks.push(Box::pin(drogue_cloud_mqtt_endpoint::run(config.clone())));
                mqtt_endpoints.push(config.clone());

                let mut config_ws = config;
                // browsers need disabled client certs
                config_ws.disable_client_certificates = true;
                config_ws.mqtt.transport = Transport::Websocket;
                config_ws.mqtt.bind_addr = Some(bind_addr_mqtt_ws);

                //tasks.push(Box::pin(drogue_cloud_mqtt_endpoint::run(config_ws)));
                mqtt_endpoints.push(config_ws);
            }

            if matches.is_present("enable-mqtt-integration") || matches.is_present("enable-all") {
                log::info!("Enabling MQTT integration");
                let bind_addr_mqtt = server.mqtt_integration.clone().into();
                let bind_addr_mqtt_ws = server.mqtt_integration_ws.clone().into();
                let kafka = server.kafka;
                let cert_bundle_file: Option<String> =
                    matches.value_of("server-cert").map(|s| s.to_string());
                let key_file: Option<String> =
                    matches.value_of("server-key").map(|s| s.to_string());
                let registry = registry.clone();
                let user_auth = user_auth.clone();
                let config = drogue_cloud_mqtt_integration::Config {
                    mqtt: MqttServerOptions {
                        workers: Some(1),
                        bind_addr: Some(bind_addr_mqtt),
                        ..Default::default()
                    },
                    health: None,
                    oauth,
                    disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                    disable_client_certificates: false,
                    cert_bundle_file,
                    key_file,
                    registry,
                    service: drogue_cloud_mqtt_integration::ServiceConfig {
                        kafka: kafka.clone(),
                        enable_username_password_auth: false,
                        disable_api_keys: false,
                    },
                    check_kafka_topic_ready: false,
                    user_auth,
                    instance: "drogue".to_string(),
                    command_kafka_sink: kafka,
                    endpoint_pool: Default::default(),
                };

                //                tasks.push(Box::pin(drogue_cloud_mqtt_integration::run(config.clone())));
                mqtt_integrations.push(config.clone());

                let mut config_ws = config;
                // browsers need disabled client certs
                config_ws.disable_client_certificates = true;
                config_ws.mqtt.transport = Transport::Websocket;
                config_ws.mqtt.bind_addr = Some(bind_addr_mqtt_ws);

                //tasks.push(Box::pin(drogue_cloud_mqtt_integration::run(config_ws)));
                mqtt_integrations.push(config_ws);
            }

            // we need to ensure that we only call select_all if we have tasks and only submit
            // tasks to "tasks" which will keep running and have a meaning.
            if !mqtt_endpoints.is_empty() && !mqtt_integrations.is_empty() {
                tasks.push(Box::pin(async {
                    Handle::current()
                        .spawn_blocking(move || {
                            let runner = ntex::rt::System::new("ntex");

                            let mut tasks: Vec<Pin<Box<dyn Future<Output = anyhow::Result<()>>>>> =
                                Vec::new();

                            for config in mqtt_endpoints {
                                tasks.push(Box::pin(drogue_cloud_mqtt_endpoint::run(config)));
                            }

                            for config in mqtt_integrations {
                                tasks.push(Box::pin(drogue_cloud_mqtt_integration::run(config)));
                            }

                            runner.block_on(async move {
                                let (result, _, _) = futures::future::select_all(tasks).await;
                                result
                            })?;

                            Ok::<(), anyhow::Error>(())
                        })
                        .await??;

                    Ok(())
                }));
            }
        }

        if matches.is_present("enable-coap-endpoint") || matches.is_present("enable-all") {
            log::info!("Enabling CoAP endpoint");
            let command_source_kafka = command_source("coap_endpoint");
            let bind_addr = server.coap.clone().into();
            let kafka = server.kafka.clone();
            let config = drogue_cloud_coap_endpoint::Config {
                auth,
                health: None,
                bind_addr_coap: Some(bind_addr),
                instance: "drogue".to_string(),
                command_source_kafka,
                kafka_downstream_config: kafka.clone(),
                kafka_command_config: kafka,
                check_kafka_topic_ready: false,
                endpoint_pool: Default::default(),
            };

            tasks.push(Box::pin(drogue_cloud_coap_endpoint::run(config)));
        }

        // The idea is to run a UI server if explicitly requested, or if a UI dist directory
        // was provided.
        let frontend = if (matches.is_present("enable-console-frontend")
            || (matches.is_present("enable-all") && matches.is_present("ui-dist")))
            && (!matches.is_present("disable-console-frontend"))
        {
            log::info!("Enable console frontend");
            let ui = matches.value_of("ui-dist").unwrap().to_string();
            let bind_addr: String = server.frontend.clone().into();

            tasks.push(Box::pin(async move {
                use drogue_cloud_service_api::webapp::App;

                HttpServer::new(move || {
                    App::new()
                        .service(actix_files::Files::new("/", ui.clone()).index_file("index.html"))
                })
                .bind(bind_addr)?
                .run()
                .await?;

                Ok(())
            }));
            true
        } else {
            false
        };

        run(
            Context {
                tls,
                mqtt_prefix,
                http_prefix,
                ws_prefix,
                frontend,
            },
            server,
            tasks,
        )
        .await;
    } else {
        log::error!("No subcommand specified");
        app.print_long_help().unwrap();
        std::process::exit(1);
    }
}

pub struct Context<'c> {
    pub mqtt_prefix: &'c str,
    pub http_prefix: &'c str,
    pub ws_prefix: &'c str,
    pub tls: bool,
    pub frontend: bool,
}

async fn run(
    ctx: Context<'_>,
    server: ServerConfig,
    mut tasks: Vec<Pin<Box<dyn Future<Output = anyhow::Result<()>>>>>,
) {
    if tasks.is_empty() {
        log::error!("No service was enabled. This server will exit now. You can enable services selectively (see --help) or just start all using --enable-all");
        return;
    }

    println!("Drogue Cloud is running!");
    println!();

    println!("Endpoints:");
    println!(
        "\tAPI:\t\t http://{}:{}",
        server.console.host, server.console.port
    );
    println!(
        "\tConsole:\t {}",
        if ctx.frontend {
            format!("http://{}:{}", server.frontend.host, server.frontend.port)
        } else {
            "<not active>".to_string()
        }
    );
    println!(
        "\tHTTP:\t\t {}://{}:{}",
        ctx.http_prefix, server.http.host, server.http.port
    );
    println!(
        "\tMQTT:\t\t {}://{}:{}",
        ctx.mqtt_prefix, server.mqtt.host, server.mqtt.port
    );
    println!(
        "\tCoAP:\t\t coap://{}:{}",
        server.coap.host, server.coap.port
    );
    println!();
    println!("Integrations:");
    println!(
        "\tWebSocket:\t {}://{}:{}",
        ctx.ws_prefix, server.websocket_integration.host, server.websocket_integration.port
    );
    println!(
        "\tMQTT:\t\t {}://{}:{}",
        ctx.mqtt_prefix, server.mqtt_integration.host, server.mqtt_integration.port
    );
    println!();
    println!("Command:");
    println!(
        "\tHTTP:\t\t http://{}:{}",
        server.command.host, server.command.port
    );
    println!();

    println!("Keycloak Credentials:");
    println!("\tUser:\t\t {}", server.keycloak.user);
    println!("\tPassword:\t {}", server.keycloak.password);
    println!();

    println!("Logging in:");
    println!(
        "\tdrg login http://{}:{}",
        server.console.host, server.console.port
    );
    println!();

    println!("Creating an application:");
    println!("\tdrg create app example-app");
    println!();

    println!("Creating a device:");
    println!("\tdrg create device --application example-app device1 --spec '{{\"credentials\":{{\"credentials\":[{{\"pass\":\"hey-rodney\"}}]}}}}'");
    println!();

    println!("Streaming telemetry data for an application:");
    println!("\tdrg stream -a example-app");
    println!();

    println!("Publishing data to the HTTP endpoint:");
    println!("\tcurl -u 'device1@example-app:hey-rodney' -d '{{\"temp\": 42}}' -v -H \"Content-Type: application/json\" -X POST {}://{}:{}/v1/telemetry", if ctx.tls { "-k https" } else {"http"}, server.http.host, server.http.port);
    println!();

    println!("Publishing data to the MQTT endpoint:");
    println!("\tmqtt pub -v -h {host} -p {port} -u 'device1@example-app' -pw 'hey-rodney' {tls} -t temp -m '{{\"temp\":42}}'",
             host = server.mqtt.host,
             port = server.mqtt.port,
             tls = if ctx.tls { "-s" } else { "" },
    );
    println!();

    // add terminate handler
    tasks.push(Box::pin(
        tokio::signal::ctrl_c()
            .map_ok(|r| {
                log::warn!("Ctrl-C pressed. Exiting application ...");
                r
            })
            .map_err(|err| anyhow!(err)),
    ));

    let (result, _, _) = futures::future::select_all(tasks).await;

    log::warn!("Shutting down as one task terminated: {result:?}");
}
