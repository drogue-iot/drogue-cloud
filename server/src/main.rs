use clap::{crate_version, App, Arg, SubCommand};
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
    pub database: Database,
    pub keycloak: Keycloak,
    pub kafka: KafkaClientConfig,
}

#[derive(Clone)]
pub struct Database {
    endpoint: Endpoint,
    db: String,
    user: String,
    password: String,
}

#[derive(Clone)]
pub struct Keycloak {
    endpoint: Endpoint,
    user: String,
    password: String,
}

impl ServerConfig {
    fn new(iface: &str) -> ServerConfig {
        ServerConfig {
            kafka: KafkaClientConfig {
                bootstrap_servers: "localhost:9092".to_string(),
                properties: HashMap::new(),
            },
            database: Database {
                endpoint: Endpoint {
                    host: "localhost".to_string(),
                    port: 5432,
                },
                db: "drogue".to_string(),
                user: "admin".to_string(),
                password: "admin123456".to_string(),
            },
            keycloak: Keycloak {
                endpoint: Endpoint {
                    host: "localhost".to_string(),
                    port: 8080,
                },
                user: "admin".to_string(),
                password: "admin123456".to_string(),
            },
            console: Endpoint {
                host: iface.to_string(),
                port: 10001,
            },
            mqtt: Endpoint {
                host: iface.to_string(),
                port: 1883,
            },
            http: Endpoint {
                host: iface.to_string(),
                port: 8088,
            },
            coap: Endpoint {
                host: iface.to_string(),
                port: 5683,
            },
            mqtt_integration: Endpoint {
                host: iface.to_string(),
                port: 18883,
            },
            websocket_integration: Endpoint {
                host: iface.to_string(),
                port: 10002,
            },
            command: Endpoint {
                host: iface.to_string(),
                port: 10003,
            },
            registry: Endpoint {
                host: iface.to_string(),
                port: 10004,
            },
            device_auth: Endpoint {
                host: iface.to_string(),
                port: 10005,
            },
            user_auth: Endpoint {
                host: iface.to_string(),
                port: 10006,
            },
        }
    }
}

fn run_migrations(db: &Database) {
    use diesel::Connection;
    println!("Migrating database schema...");
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db.user, db.password, db.endpoint.host, db.endpoint.port, db.db
    );
    let connection = diesel::PgConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url));

    embedded_migrations::run_with_output(&connection, &mut std::io::stdout()).unwrap();
    println!("Migrating database schema... done!");
}

fn configure_keycloak(server: &Keycloak) {
    print!("Configuring keycloak... ");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let url = format!("http://{}:{}", server.endpoint.host, server.endpoint.port);
        let user = server.user.clone();
        let password = server.password.clone();
        let client = reqwest::Client::new();
        let admin_token = keycloak::KeycloakAdminToken::acquire(&url, &user, &password, &client)
            .await
            .unwrap();

        let admin = keycloak::KeycloakAdmin::new(&url, admin_token, client);
        let mut c: keycloak::types::ClientRepresentation = Default::default();
        c.client_id.replace("drogue".to_string());
        c.enabled.replace(true);
        c.redirect_uris.replace(vec!["*".to_string()]);
        c.web_origins.replace(vec!["*".to_string()]);
        c.client_authenticator_type
            .replace("client-secret".to_string());
        c.public_client.replace(true);

        match admin.realm_clients_post("master", c).await {
            Ok(_) => {
                println!("done!");
            }
            Err(e) => {
                if let keycloak::KeycloakError::HttpFailure {
                    status: 409,
                    body: _,
                    text: _,
                } = e
                {
                    log::trace!("Client already exists");
                    println!("done!");
                } else {
                    log::warn!("Error creating keycloak client: {:?}", e);
                    println!("failed!");
                }
            }
        }
    });
}

fn main() {
    //env_logger::init();
    dotenv::dotenv().ok();
    let mut app = App::new("Drogue Cloud Server")
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
                ),
        );

    let matches = app.clone().get_matches();

    stderrlog::new()
        .verbosity((matches.occurrences_of("verbose") + 1) as usize)
        .quiet(matches.is_present("quiet"))
        .init()
        .unwrap();

    if let Some(matches) = matches.subcommand_matches("run") {
        let server: ServerConfig = matches
            .value_of("bind-address")
            .map(|a| ServerConfig::new(a))
            .unwrap_or_else(|| ServerConfig::new("localhost"));
        let eps = endpoints(&server);

        run_migrations(&server.database);

        configure_keycloak(&server.keycloak);
        /*
        let kafka_stream = |topic: &str, consumer_group: &str| KafkaStreamConfig {
            client: kafka_config(topic),
            consumer_group: consumer_group.to_string(),
        };
        */

        let kafka_sender = |topic: &str, config: &KafkaClientConfig| KafkaSenderConfig {
            client: kafka_config(config, topic),
            queue_timeout: None,
        };

        let command_source = |consumer_group: &str| KafkaCommandSourceConfig {
            topic: "iot-commands".to_string(),
            consumer_group: consumer_group.to_string(),
        };

        let oauth = AuthenticatorConfig {
            disabled: true,
            global: AuthenticatorGlobalConfig {
                sso_url: eps.sso.clone(),
                issuer_url: eps.issuer_url.clone(),
                realm: "master".to_string(),
                redirect_url: None,
            },
            clients: HashMap::new(),
        };

        let keycloak = KeycloakAdminClientConfig {
            url: Url::parse(&eps.sso.as_ref().unwrap()).unwrap(),
            realm: "master".into(),
            admin_username: server.keycloak.user.clone(),
            admin_password: server.keycloak.password.clone(),
            tls_noverify: true,
        };

        let registry = RegistryConfig {
            url: Url::parse(&eps.registry.as_ref().unwrap().url).unwrap(),
            token_config: None,
        };

        let mut pg = deadpool_postgres::Config::new();
        pg.host = Some(server.database.endpoint.host.clone());
        pg.port = Some(server.database.endpoint.port);
        pg.user = Some(server.database.user.clone());
        pg.password = Some(server.database.password.clone());
        pg.dbname = Some(server.database.db.clone());
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
            issuer_url: eps.issuer_url.as_ref().map(|u| Url::parse(u).unwrap()),
            sso_url: Url::parse(&eps.sso.as_ref().unwrap()).ok(),
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
                    kafka_sender: kafka_sender("registry", &s.kafka),
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
                    kafka: s.kafka.clone(),
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
            let bind_addr = server.http.clone().into();
            let kafka = server.kafka.clone();
            let cert_bundle_file: Option<String> =
                matches.value_of("server-cert").map(|s| s.to_string());
            let key_file: Option<String> = matches.value_of("server-key").map(|s| s.to_string());

            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_http_endpoint::Config {
                    auth: a,
                    disable_tls: !(key_file.is_some() && cert_bundle_file.is_some()),
                    health: None,
                    max_json_payload_size: 65536,
                    max_payload_size: 65536,
                    cert_bundle_file,
                    key_file,
                    command_source_kafka,
                    instance: "drogue".to_string(),
                    kafka_downstream_config: kafka.clone(),
                    kafka_command_config: kafka,
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

        if matches.is_present("enable-mqtt-endpoint") || matches.is_present("enable-all") {
            log::info!("Enabling MQTT endpoint");
            let a = auth.clone();
            let command_source_kafka = command_source("mqtt_endpoint");
            let bind_addr_mqtt = server.mqtt.clone().into();
            let kafka = server.kafka.clone();
            threads.push(std::thread::spawn(move || {
                let config = drogue_cloud_mqtt_endpoint::Config {
                    auth: a,
                    health: None,
                    disable_tls: true,
                    cert_bundle_file: None,
                    key_file: None,
                    bind_addr_mqtt: Some(bind_addr_mqtt),
                    instance: "drogue".to_string(),
                    command_source_kafka,
                    kafka_downstream_config: kafka.clone(),
                    kafka_command_config: kafka,
                    check_kafka_topic_ready: false,
                };

                ntex::rt::System::new("mqtt-endpoint")
                    .block_on(drogue_cloud_mqtt_endpoint::run(config))
                    .unwrap();
            }));
        }

        println!("Drogue Cloud is running!");
        println!("");

        println!("Endpoints:");
        println!(
            "\tAPI:\t http://{}:{}",
            server.console.host, server.console.port
        );
        println!("\tHTTP:\t http://{}:{}", server.http.host, server.http.port);
        println!("\tMQTT:\t mqtt://{}:{}", server.mqtt.host, server.mqtt.port);
        println!("");

        println!("Keycloak Credentials:");
        println!("\tUser: {}", server.keycloak.user);
        println!("\tPassword: {}", server.keycloak.password);
        println!("");

        println!("Logging in:");
        println!(
            "\tdrg login http://{}:{}",
            server.console.host, server.console.port
        );
        println!("");

        println!("Creating an application:");
        println!("\tdrg create app example-app");
        println!("");

        println!("Creating a device:");
        println!("\tdrg create device --app example-app device1 --spec '{{\"credentials\":{{\"credentials\":[{{\"pass\":\"hey-rodney\"}}]}}}}'");
        println!("");

        println!("Publishing data to the HTTP endpoint:");
        println!("\tcurl -u 'device1@example-app:hey-rodney' -d '{{\"temp\": 42}}' -v -H \"Content-Type: application/json\" -X POST {}://{}:{}/v1/foo", if matches.is_present("server-cert") && matches.is_present("server-key") { "-k https" } else {"http"}, server.http.host, server.http.port);
        println!("");

        if threads.is_empty() {
            log::warn!("No services selected to start up. Process will exit. Enable some services using --enable-* or enable all using --enable-all.")
        }

        for t in threads.drain(..) {
            t.join().unwrap();
        }
        log::info!("All services stopped");
    } else {
        log::error!("No subcommand specified");
        app.print_long_help().unwrap();
        std::process::exit(1);
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
        sso: Some(format!(
            "http://{}:{}",
            config.keycloak.endpoint.host, config.keycloak.endpoint.port
        )),
        issuer_url: Some(format!(
            "http://{}:{}/auth/realms/master",
            config.keycloak.endpoint.host, config.keycloak.endpoint.port
        )),
        redirect_url: Some(format!(
            "http://{}:{}",
            config.console.host, config.console.port
        )),
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

fn kafka_config(kafka: &KafkaClientConfig, topic: &str) -> KafkaConfig {
    KafkaConfig {
        client: kafka.clone(),
        topic: topic.to_string(),
    }
}
