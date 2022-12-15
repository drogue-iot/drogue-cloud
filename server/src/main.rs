mod config;
mod db;
mod keycloak;

use crate::{config::*, keycloak::*};
use anyhow::anyhow;
use clap::{crate_version, value_parser, Arg, ArgAction, ArgMatches, Command};
use drogue_cloud_authentication_service::service::AuthenticationServiceConfig;
use drogue_cloud_command_routing_service::service::postgres::PostgresServiceConfiguration as CommandRoutingPostgresServiceConfiguration;
use drogue_cloud_database_common::postgres;
use drogue_cloud_device_management_service::service::PostgresManagementServiceConfig;
use drogue_cloud_device_state_service::service::postgres::PostgresServiceConfiguration as DeviceStatePostgresServiceConfiguration;
use drogue_cloud_endpoint_common::{auth::AuthConfig, command::KafkaCommandSourceConfig};
use drogue_cloud_mqtt_common::server::{MqttServerOptions, Transport};
use drogue_cloud_registry_events::sender::KafkaSenderConfig; //, stream::KafkaStreamConfig};
use drogue_cloud_service_api::{kafka::KafkaClientConfig, webapp::HttpServer};
use drogue_cloud_service_common::actix::http::CorsConfig;
use drogue_cloud_service_common::client::CommandRoutingClientConfig;
use drogue_cloud_service_common::command_routing::CommandRoutingControllerConfiguration;
use drogue_cloud_service_common::{
    actix::http::{CorsBuilder, HttpBuilder, HttpConfig},
    app::{Main, Startup, StartupExt, SubMain},
    auth::openid::{
        AuthenticatorClientConfig, AuthenticatorConfig, AuthenticatorGlobalConfig, TokenConfig,
    },
    client::{ClientConfig, DeviceStateClientConfig},
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig},
    state::StateControllerConfiguration,
};
use drogue_cloud_user_auth_service::service::AuthorizationServiceConfig;
use futures::TryFutureExt;
use std::{collections::HashMap, time::Duration};
use tokio::runtime::Handle;
use url::Url;

fn args() -> Command {
    Command::new("Drogue Cloud Server")
        .about("Running Drogue Cloud in a single process")
        .version(crate_version!())
        .long_about("Drogue Server runs all the Drogue Cloud services in a single process, with an external dependency on PostgreSQL, Kafka and Keycloak for storing data, device management and user management")
        .arg(
            Arg::new("verbose")
                .global(true)
                .long("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Be verbose. Can be used multiple times to increase verbosity.")
        )
        .arg(
            Arg::new("quiet")
                .global(true)
                .long("quiet")
                .short('q')
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("verbose")
                .help("Be quiet.")
        )
        .subcommand(
            Command::new("run")
                .about("run server")
                .arg(
                    Arg::new("insecure")
                        .long("insecure")
                        .action(ArgAction::SetTrue)
                        .help("Run insecure, like disabling TLS checks")
                )
                .arg(
                    Arg::new("bind-address")
                        .long("bind-address")
                        .help("bind to specific network address (default localhost)")
                        .value_name("ADDRESS"),
                )
                .arg(
                    Arg::new("enable-all")
                        .long("enable-all")
                        .action(ArgAction::SetTrue)
                        .help("enable all services (except console-frontend)"),
                )
                .arg(
                    Arg::new("enable-api")
                        .long("enable-api")
                        .action(ArgAction::SetTrue)
                        .help("enable API backend service"),
                )
                .arg(
                    Arg::new("enable-console-frontend")
                        .long("enable-console-frontend")
                        .action(ArgAction::SetTrue)
                        .requires("ui-dist")
                        .help("enable console frontend serving"),
                )
                .arg(
                    // helps when enable-all is active, but you don't want the console
                    Arg::new("disable-console-frontend")
                        .long("disable-console-frontend")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("enable-console-frontend")
                        .conflicts_with("ui-dist")
                        .help("disable console frontend serving"),
                )
                .arg(
                    Arg::new("enable-command-routing")
                        .long("enable-command-routing")
                        .action(ArgAction::SetTrue)
                        .help("enable command routing service"),
                )
                .arg(
                    Arg::new("enable-device-registry")
                        .long("enable-device-registry")
                        .action(ArgAction::SetTrue)
                        .help("enable device management service"),
                )
                .arg(
                    Arg::new("enable-device-state")
                        .long("enable-device-state")
                        .action(ArgAction::SetTrue)
                        .help("enable device state service"),
                )
                .arg(
                    Arg::new("enable-user-authentication-service")
                        .long("enable-user-authentication-service")
                        .action(ArgAction::SetTrue)
                        .help("enable user authentication service"),
                )
                .arg(
                    Arg::new("enable-authentication-service")
                        .long("enable-authentication-service")
                        .action(ArgAction::SetTrue)
                        .help("enable device authentication service"),
                )
                .arg(
                    Arg::new("enable-coap-endpoint")
                        .long("enable-coap-endpoint")
                        .action(ArgAction::SetTrue)
                        .help("enable coap endpoint"),
                )
                .arg(
                    Arg::new("enable-http-endpoint")
                        .long("enable-http-endpoint")
                        .action(ArgAction::SetTrue)
                        .help("enable http endpoint"),
                )
                .arg(
                    Arg::new("enable-mqtt-endpoint")
                        .long("enable-mqtt-endpoint")
                        .action(ArgAction::SetTrue)
                        .help("enable mqtt endpoint"),
                )
                .arg(
                    Arg::new("enable-mqtt-integration")
                        .long("enable-mqtt-integration")
                        .action(ArgAction::SetTrue)
                        .help("enable mqtt integration"),
                )
                .arg(
                    Arg::new("enable-websocket-integration")
                        .long("enable-websocket-integration")
                        .action(ArgAction::SetTrue)
                        .help("enable websocket integration"),
                )
                .arg(
                    Arg::new("enable-command-endpoint")
                        .long("enable-command-endpoint")
                        .action(ArgAction::SetTrue)
                        .help("enable command endpoint"),
                )
                .arg(
                    Arg::new("server-key")
                        .long("server-key")
                        .value_name("FILE")
                        .help("private key to use for service endpoints"),
                )
                .arg(
                    Arg::new("server-cert")
                        .long("server-cert")
                        .value_name("FILE")
                        .help("public certificate to use for service endpoints"),
                )
                .arg(
                    Arg::new("database-host")
                        .long("database-host")
                        .value_name("HOST")
                        .help("hostname of PostgreSQL database"),
                )
                .arg(
                    Arg::new("database-port")
                        .long("database-port")
                        .value_parser(value_parser!(u16))
                        .value_name("PORT")
                        .help("port of PostgreSQL database"),
                )
                .arg(
                    Arg::new("database-name")
                        .long("database-name")
                        .value_name("NAME")
                        .help("name of database to use"),
                )
                .arg(
                    Arg::new("database-user")
                        .long("database-user")
                        .value_name("USER")
                        .help("username to use with database"),
                )
                .arg(
                    Arg::new("database-password")
                        .long("database-password")
                        .value_name("PASSWORD")
                        .help("password to use with database"),
                )
                .arg(
                    Arg::new("keycloak-url")
                        .long("keycloak-url")
                        .value_name("URL")
                        .help("url for Keycloak"),
                )
                .arg(
                    Arg::new("keycloak-realm")
                        .long("keycloak-realm")
                        .value_name("REALM")
                        .help("Keycloak realm to use"),
                )
                .arg(
                    Arg::new("keycloak-user")
                        .long("keycloak-user")
                        .value_name("USER")
                        .help("Keycloak realm admin user"),
                )
                .arg(
                    Arg::new("keycloak-password")
                        .long("keycloak-password")
                        .value_name("PASSWORD")
                        .help("Keycloak realm admin password"),
                )
                .arg(
                    Arg::new("drogue-admin-user")
                        .long("drogue-admin-user")
                        .value_name("USER")
                        .help("Drogue admin user"),
                )
                .arg(
                    Arg::new("drogue-admin-password")
                        .long("drogue-admin-password")
                        .value_name("PASSWORD")
                        .help("Drogue admin password"),
                )
                .arg(
                    Arg::new("kafka-bootstrap-servers")
                        .long("kafka-bootstrap-servers")
                        .value_name("HOSTS")
                        .help("Kafka bootstrap servers"),
                )
                .arg(
                    Arg::new("ui-dist")
                        .long("ui-dist")
                        .value_name("PATH")
                        .env("UI_DIST")
                        .help("Path to the UI distribution bundle")
                ),
        )
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let mut app = args();
    let matches = app.clone().get_matches();

    stderrlog::new()
        .verbosity((matches.get_count("verbose") + 1) as usize)
        .quiet(matches.get_flag("quiet"))
        .init()
        .unwrap();

    if let Some(matches) = matches.subcommand_matches("run") {
        cmd_run(matches).await.unwrap();
    } else {
        log::error!("No subcommand specified");
        app.print_long_help().unwrap();
        std::process::exit(1);
    }
}

async fn cmd_run(matches: &ArgMatches) -> anyhow::Result<()> {
    let mut main = Main::from_env()?;

    let tls = matches.get_one::<String>("server-cert").is_some()
        && matches.get_one::<String>("server-key").is_some();
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
    let coap_prefix = if tls { "coaps" } else { "coap" };
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
        issuer_url: eps
            .issuer_url
            .as_ref()
            .map(|u| Url::parse(u).unwrap())
            .expect("Requires issuer_url"),
        refresh_before: None,
        tls_insecure: server.tls_insecure,
        tls_ca_certificates: server.tls_ca_certificates.clone().into(),
    };

    let mut oauth = AuthenticatorConfig {
        disabled: false,
        global: AuthenticatorGlobalConfig {
            issuer_url: eps.issuer_url.clone(),
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
        url: Url::parse(&server.keycloak.url)?,
        realm: server.keycloak.realm.clone(),
        admin_username: server.keycloak.user.clone(),
        admin_password: server.keycloak.password.clone(),
        tls_insecure: server.tls_insecure,
        tls_ca_certificates: server.tls_ca_certificates.clone().into(),
    };

    let registry = ClientConfig {
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

    let user_auth = Some(ClientConfig {
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

    let command_router_client = CommandRoutingClientConfig {
        url: Url::parse(&format!(
            "http://{}:{}",
            server.command_routing.host, server.command_routing.port
        ))
        .unwrap(),
        token_config: Some(token_config.clone()),
        ..Default::default()
    };

    let command_router = CommandRoutingControllerConfiguration {
        client: command_router_client.clone(),
        init_delay: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let oauth = oauth.clone();
    let server = server.clone();
    let auth = auth.clone();
    let registry = registry.clone();
    let matches = matches.clone();
    let user_auth = user_auth.clone();

    if matches.get_flag("enable-device-state") || matches.get_flag("enable-all") {
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

            oauth: oauth.clone(),
            service: DeviceStatePostgresServiceConfiguration {
                session_timeout: Duration::from_secs(10),
                pg: pg.clone(),
            },
            instance: "drogue".to_string(),
            check_kafka_topic_ready: false,
            kafka_downstream_config: kafka,
            endpoint_pool: Default::default(),
            registry: registry.clone(),
        };

        drogue_cloud_device_state_service::run(config, &mut main).await?;
    }

    if matches.get_flag("enable-command-routing") || matches.get_flag("enable-all") {
        log::info!("Enabling command routing service");

        let config = drogue_cloud_command_routing_service::Config {
            http: HttpConfig {
                bind_addr: server.command_routing.clone().into(),
                disable_tls: true,
                workers: Some(1),
                metrics_namespace: Some("command_routing_service".into()),
                ..Default::default()
            },

            oauth: oauth.clone(),
            service: CommandRoutingPostgresServiceConfiguration {
                session_timeout: Duration::from_secs(10),
                pg: pg.clone(),
            },
            instance: "drogue".to_string(),
            registry: registry.clone(),
        };

        drogue_cloud_command_routing_service::run(config, &mut main).await?;
    }

    if matches.get_flag("enable-user-authentication-service") || matches.get_flag("enable-all") {
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
            service: AuthorizationServiceConfig { pg: pg.clone() },
        };

        drogue_cloud_user_auth_service::run::<KeycloakAdminClient>(config, &mut main).await?;
    }

    if matches.get_flag("enable-authentication-service") || matches.get_flag("enable-all") {
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
            auth_service_config: AuthenticationServiceConfig { pg: pg.clone() },
        };

        drogue_cloud_authentication_service::run(config, &mut main).await?;
    }

    if matches.get_flag("enable-api") || matches.get_flag("enable-all") {
        log::info!("Enabling composite API service");
        let console_config = {
            let mut console_token_config = token_config.clone();
            console_token_config.client_id = "drogue".to_string();
            drogue_cloud_console_backend::Config {
                http: Default::default(), // overridden later on
                oauth: oauth.clone(),
                enable_kube: false,
                kafka: server.kafka.clone(),
                keycloak: keycloak.clone(),
                registry: registry.clone(),
                console_token_config: Some(console_token_config),
                disable_account_url: false,
                scopes: "openid profile email".into(),
                user_auth: user_auth.clone(),
            }
        };

        let config_device_management_service = drogue_cloud_device_management_service::Config {
            http: Default::default(), // overridden later on
            user_auth: user_auth.clone(),
            oauth: oauth.clone(),
            keycloak: keycloak.clone(),
            database_config: PostgresManagementServiceConfig {
                pg: pg.clone(),
                instance: server.database.db.to_string(),
            },
            kafka_sender: kafka_sender("registry", &server.kafka.clone()),
        };

        let config_command = {
            let kafka = server.kafka.clone();
            let user_auth = user_auth.clone();

            drogue_cloud_command_endpoint::Config {
                http: Default::default(), // overridden later on
                oauth: oauth.clone(),
                registry: registry.clone(),
                instance: "drogue".to_string(),
                check_kafka_topic_ready: false,
                command_kafka_sink: kafka,
                user_auth,
                endpoint_pool: Default::default(),
                command_routing_client: command_router_client.clone(),
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

        let (registry, _) =
            drogue_cloud_device_management_service::configurator(config_device_management_service)
                .await
                .unwrap();

        let (command, _) = drogue_cloud_command_endpoint::configurator(config_command)
            .await
            .unwrap();

        HttpBuilder::new(http, Some(main.runtime_config()), move |cfg| {
            console_backend(cfg);
            registry(cfg);
            command(cfg);
        })
        .cors(CorsBuilder::Permissive)
        .start(&mut main)?;
    }

    if matches.get_flag("enable-http-endpoint") || matches.get_flag("enable-all") {
        log::info!("Enabling HTTP endpoint");
        let command_source_kafka = command_source("http_endpoint");
        let kafka = server.kafka.clone();
        let cert_bundle_file = matches.get_one::<String>("server-cert").cloned();
        let key_file = matches.get_one("server-key").cloned();

        let config = drogue_cloud_http_endpoint::Config {
            http: HttpConfig {
                workers: Some(1),
                disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                cert_bundle_file,
                key_file,
                bind_addr: server.http.clone().into(),
                metrics_namespace: Some("http_endpoint".into()),
                cors: CorsConfig {
                    allow_any_origin: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            auth: auth.clone(),
            command_source_kafka,
            instance: "drogue".to_string(),
            kafka_downstream_config: kafka.clone(),
            kafka_command_config: kafka,
            check_kafka_topic_ready: false,
            endpoint_pool: Default::default(),
        };

        drogue_cloud_http_endpoint::run(config, &mut main).await?;
    }

    if matches.get_flag("enable-websocket-integration") || matches.get_flag("enable-all") {
        log::info!("Enabling Websocket integration");
        let bind_addr = server.websocket_integration.clone().into();
        let cert_bundle_file = matches.get_one("server-cert").cloned();
        let key_file: Option<String> = matches.get_one("server-key").cloned();
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
            oauth: oauth.clone(),

            registry: registry.clone(),
            kafka,
            user_auth,
        };

        // The websocket integration uses the actix actors, so for now, that must run
        // on an actix runtime.
        let sub_main = main.sub_main_seed();
        main.spawn(async move {
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
                        let mut sub_main: SubMain = sub_main.into();
                        drogue_cloud_websocket_integration::run(config, &mut sub_main).await?;
                        sub_main.run().await
                    })
                })
                .await??;

            Ok::<(), anyhow::Error>(())
        });
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
        let cert_bundle_file = matches.get_one("server-cert").cloned();
        let key_file = matches.get_one("server-key").cloned();

        let mut mqtt_endpoints: Vec<drogue_cloud_mqtt_endpoint::Config> = vec![];
        let mut mqtt_integrations: Vec<drogue_cloud_mqtt_integration::Config> = vec![];

        if matches.get_flag("enable-mqtt-endpoint") || matches.get_flag("enable-all") {
            log::info!("Enabling MQTT endpoint");

            let config = drogue_cloud_mqtt_endpoint::Config {
                mqtt: MqttServerOptions {
                    workers: Some(1),
                    bind_addr: Some(bind_addr_mqtt),
                    ..Default::default()
                },
                command_http: HttpConfig {
                    bind_addr: "localhost:20002".into(),
                    disable_tls: true,
                    workers: Some(1),
                    ..Default::default()
                },

                oauth: oauth.clone(),
                endpoint: Default::default(),
                auth,
                disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                disable_client_certificates: false,
                disable_tls_psk: false,
                cert_bundle_file,
                key_file,
                instance: "drogue".to_string(),
                command_source_kafka,
                kafka_downstream_config: kafka.clone(),
                kafka_command_config: kafka,
                check_kafka_topic_ready: false,
                endpoint_pool: Default::default(),
                state: state.clone(),
                command_routing: command_router.clone(),
            };

            mqtt_endpoints.push(config.clone());

            let mut config_ws = config;
            // browsers need disabled client certs
            config_ws.disable_client_certificates = true;
            config_ws.mqtt.transport = Transport::Websocket;
            config_ws.mqtt.bind_addr = Some(bind_addr_mqtt_ws);

            mqtt_endpoints.push(config_ws);
        }

        if matches.get_flag("enable-mqtt-integration") || matches.get_flag("enable-all") {
            log::info!("Enabling MQTT integration");
            let bind_addr_mqtt = server.mqtt_integration.clone().into();
            let bind_addr_mqtt_ws = server.mqtt_integration_ws.clone().into();
            let kafka = server.kafka;
            let cert_bundle_file = matches.get_one("server-cert").cloned();
            let key_file = matches.get_one("server-key").cloned();
            let registry = registry.clone();
            let user_auth = user_auth.clone();
            let config = drogue_cloud_mqtt_integration::Config {
                mqtt: MqttServerOptions {
                    workers: Some(1),
                    bind_addr: Some(bind_addr_mqtt),
                    ..Default::default()
                },
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

            // tasks.push(Box::pin(drogue_cloud_mqtt_integration::run(config.clone())));
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
            let sub_main = main.sub_main_seed();
            main.spawn(async {
                Handle::current()
                    .spawn_blocking(move || {
                        let runner = ntex::rt::System::new("ntex");

                        runner.block_on(async move {
                            let mut sub_main: SubMain = sub_main.into();

                            for config in mqtt_endpoints {
                                drogue_cloud_mqtt_endpoint::run(config, &mut sub_main).await?;
                            }

                            for config in mqtt_integrations {
                                drogue_cloud_mqtt_integration::run(config, &mut sub_main).await?;
                            }

                            sub_main.run().await
                        })?;

                        Ok::<(), anyhow::Error>(())
                    })
                    .await??;

                Ok(())
            });
        }
    }

    if matches.get_flag("enable-coap-endpoint") || matches.get_flag("enable-all") {
        log::info!("Enabling CoAP endpoint");
        let command_source_kafka = command_source("coap_endpoint");
        let bind_addr = server.coap.clone().into();
        let kafka = server.kafka.clone();
        let cert_bundle_file = matches.get_one("server-cert").cloned();
        let key_file = matches.get_one("server-key").cloned();
        let config = drogue_cloud_coap_endpoint::Config {
            auth,
            bind_addr_coap: Some(bind_addr),
            instance: "drogue".to_string(),
            command_source_kafka,
            kafka_downstream_config: kafka.clone(),
            kafka_command_config: kafka,
            check_kafka_topic_ready: false,
            endpoint_pool: Default::default(),
            disable_dtls: !(key_file.is_some() && cert_bundle_file.is_some()),
            disable_client_certificates: false,
            disable_psk: false,
            dtls_session_timeout: None,
            cert_bundle_file,
            key_file,
        };

        drogue_cloud_coap_endpoint::run(config, &mut main).await?;
    }

    // The idea is to run a UI server if explicitly requested, or if a UI dist directory
    // was provided.
    let frontend = if (matches.get_flag("enable-console-frontend")
        || (matches.get_flag("enable-all") && matches.get_one::<String>("ui-dist").is_some()))
        && (!matches.get_flag("disable-console-frontend"))
    {
        log::info!("Enable console frontend");
        let ui: String = matches.get_one("ui-dist").cloned().unwrap();
        let bind_addr: String = server.frontend.clone().into();

        main.spawn(async move {
            use drogue_cloud_service_api::webapp::App;

            HttpServer::new(move || {
                App::new()
                    .service(actix_files::Files::new("/", ui.clone()).index_file("index.html"))
            })
            .bind(bind_addr)?
            .run()
            .await?;

            Ok(())
        });
        true
    } else {
        false
    };

    run(
        Context {
            tls,
            mqtt_prefix,
            http_prefix,
            coap_prefix,
            ws_prefix,
            frontend,
        },
        server,
        main,
    )
    .await?;

    Ok(())
}

pub struct Context<'c> {
    pub mqtt_prefix: &'c str,
    pub http_prefix: &'c str,
    pub coap_prefix: &'c str,
    pub ws_prefix: &'c str,
    pub tls: bool,
    pub frontend: bool,
}

async fn run(ctx: Context<'_>, server: ServerConfig, mut main: Main<'_>) -> anyhow::Result<()> {
    if main.is_empty() {
        log::error!("No service was enabled. This server will exit now. You can enable services selectively (see --help) or just start all using --enable-all");
        return Ok(());
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
        "\tCoAP:\t\t {}://{}:{}",
        ctx.coap_prefix, server.coap.host, server.coap.port
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
    println!("\tdrg create device --application example-app device1 --spec '{{\"authentication\":{{\"credentials\":[{{\"pass\":\"hey-rodney\"}}]}}}}'");
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

    println!("Publishing data to the CoAP endpoint:");
    println!("\techo -n 1000 | coap-client -m post -O 4209,\"Basic ZGV2aWNlMUBleGFtcGxlLWFwcDpoZXktcm9kbmV5\" {tls} {scheme}://{host}:{port}/v1/foo -f -",
             scheme = if ctx.tls { "coaps" } else {"coap"},
             host = server.coap.host,
             port = server.coap.port,
             tls = if ctx.tls { "-n" } else { "" },
    );
    println!();

    println!("Subscribing MQTT device to receive commands:");
    println!("\tmqtt sub -v -h {host} -p {port} -u 'device1@example-app' -pw 'hey-rodney' {tls} -t command/inbox//#",
             host = server.mqtt.host,
             port = server.mqtt.port,
             tls = if ctx.tls { "-s" } else { "" },
    );
    println!();

    println!("Sending command to the device:");
    println!("\tdrg cmd device1 set-temp --application example-app --payload \"{{\\\"target-temp\\\":25}}\"");
    println!();

    // add terminate handler
    main.spawn(
        tokio::signal::ctrl_c()
            .map_ok(|r| {
                log::warn!("Ctrl-C pressed. Exiting application ...");
                r
            })
            .map_err(|err| anyhow!(err)),
    );

    main.run().await?;

    Ok(())
}

#[test]
fn verify_app() {
    args().debug_assert();
}
