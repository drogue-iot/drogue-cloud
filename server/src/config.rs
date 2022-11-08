use clap::ArgMatches;
use core::str::FromStr;
use drogue_cloud_service_api::{
    endpoints::*,
    kafka::{KafkaClientConfig, KafkaConfig},
};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl From<Endpoint> for String {
    fn from(endpoint: Endpoint) -> Self {
        format!("{}:{}", endpoint.host, endpoint.port)
    }
}

#[derive(Clone)]
pub struct ServerConfig {
    pub console: Endpoint,
    pub frontend: Endpoint,
    pub mqtt: Endpoint,
    pub mqtt_ws: Endpoint,
    pub mqtt_ws_browser: Endpoint,
    pub http: Endpoint,
    pub coap: Endpoint,
    pub mqtt_integration: Endpoint,
    pub mqtt_integration_ws: Endpoint,
    pub mqtt_integration_ws_browser: Endpoint,
    pub websocket_integration: Endpoint,
    pub command: Endpoint,
    pub registry: Endpoint,
    pub device_auth: Endpoint,
    pub user_auth: Endpoint,
    pub device_state: Endpoint,
    pub database: Database,
    pub drogue: Drogue,
    pub keycloak: Keycloak,
    pub kafka: KafkaClientConfig,
    pub tls_insecure: bool,
    pub tls_ca_certificates: Vec<String>,
}

#[derive(Clone)]
pub struct Database {
    pub endpoint: Endpoint,
    pub db: String,
    pub user: String,
    pub password: String,
}

#[derive(Clone)]
pub struct Keycloak {
    pub url: String,
    pub realm: String,
    pub user: String,
    pub password: String,
}

#[derive(Clone)]
pub struct Drogue {
    pub admin_user: String,
    pub admin_password: String,
}

impl ServerConfig {
    pub fn new(matches: &ArgMatches) -> ServerConfig {
        let iface = matches
            .value_of("bind-address")
            .unwrap_or("localhost")
            .to_string();
        ServerConfig {
            tls_insecure: matches.is_present("insecure"),
            tls_ca_certificates: vec![],
            kafka: KafkaClientConfig {
                bootstrap_servers: matches
                    .value_of("kafka-bootstrap-servers")
                    .unwrap_or("localhost:9092")
                    .to_string(),
                properties: HashMap::new(),
            },
            database: Database {
                endpoint: Endpoint {
                    host: matches
                        .value_of("database-host")
                        .unwrap_or("localhost")
                        .to_string(),
                    port: u16::from_str(matches.value_of("database-port").unwrap_or("5432"))
                        .unwrap(),
                },
                db: matches
                    .value_of("database-name")
                    .unwrap_or("drogue")
                    .to_string(),
                user: matches
                    .value_of("database-user")
                    .unwrap_or("admin")
                    .to_string(),
                password: matches
                    .value_of("database-password")
                    .unwrap_or("admin123456")
                    .to_string(),
            },
            keycloak: Keycloak {
                url: matches
                    .value_of("keycloak-url")
                    .unwrap_or("http://localhost:8081")
                    .to_string(),
                realm: matches
                    .value_of("keycloak-realm")
                    .unwrap_or("drogue")
                    .to_string(),
                user: matches
                    .value_of("keycloak-user")
                    .unwrap_or("admin")
                    .to_string(),
                password: matches
                    .value_of("keycloak-password")
                    .unwrap_or("admin123456")
                    .to_string(),
            },
            drogue: Drogue {
                admin_user: matches
                    .value_of("drogue-admin-user")
                    .unwrap_or("admin")
                    .to_string(),
                admin_password: matches
                    .value_of("drogue-admin-password")
                    .unwrap_or("admin123456")
                    .to_string(),
            },
            frontend: Endpoint {
                host: iface.to_string(),
                port: 8010,
            },
            console: Endpoint {
                host: iface.to_string(),
                port: 8011,
            },
            mqtt: Endpoint {
                host: iface.to_string(),
                port: if matches.is_present("server-cert") && matches.is_present("server-key") {
                    8883
                } else {
                    1883
                },
            },
            mqtt_ws: Endpoint {
                host: iface.to_string(),
                port: if matches.is_present("server-cert") && matches.is_present("server-key") {
                    20443
                } else {
                    20880
                },
            },
            mqtt_ws_browser: Endpoint {
                host: iface.to_string(),
                port: if matches.is_present("server-cert") && matches.is_present("server-key") {
                    21443
                } else {
                    21880
                },
            },
            http: Endpoint {
                host: iface.to_string(),
                port: 8088,
            },
            coap: Endpoint {
                host: iface.to_string(),
                port: if matches.is_present("server-cert") && matches.is_present("server-key") {
                    5684
                } else {
                    5683
                },
            },
            mqtt_integration: Endpoint {
                host: iface.to_string(),
                port: 18883,
            },
            mqtt_integration_ws: Endpoint {
                host: iface.to_string(),
                port: 10443,
            },
            mqtt_integration_ws_browser: Endpoint {
                host: iface.to_string(),
                port: 11443,
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
            device_state: Endpoint {
                host: iface,
                port: 10007,
            },
        }
    }
}

pub fn endpoints(config: &ServerConfig, tls: bool) -> Endpoints {
    let http_prefix = if tls { "https" } else { "http" };
    let ws_prefix = if tls { "wss" } else { "ws" };
    let api = format!("http://{}:{}", config.console.host, config.console.port);
    Endpoints {
        api: Some(api.clone()),
        console: Some(format!(
            "http://{}:{}",
            config.frontend.host, config.frontend.port
        )),
        coap: Some(CoapEndpoint {
            url: format!("coap://{}:{}", config.coap.host, config.coap.port),
        }),
        http: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                http_prefix, config.http.host, config.http.port
            ),
        }),
        mqtt: Some(MqttEndpoint {
            host: config.mqtt.host.clone(),
            port: config.mqtt.port,
        }),
        mqtt_ws: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                http_prefix, config.mqtt_ws.host, config.mqtt_ws.port
            ),
        }),
        mqtt_ws_browser: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                http_prefix, config.mqtt_ws_browser.host, config.mqtt_ws_browser.port
            ),
        }),
        mqtt_integration: Some(MqttEndpoint {
            host: config.mqtt_integration.host.clone(),
            port: config.mqtt_integration.port,
        }),
        mqtt_integration_ws: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                http_prefix, config.mqtt_integration_ws.host, config.mqtt_integration_ws.port
            ),
        }),
        mqtt_integration_ws_browser: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                http_prefix,
                config.mqtt_integration_ws_browser.host,
                config.mqtt_integration_ws_browser.port
            ),
        }),
        websocket_integration: Some(HttpEndpoint {
            url: format!(
                "{}://{}:{}",
                ws_prefix, config.websocket_integration.host, config.websocket_integration.port
            ),
        }),
        issuer_url: Some(format!(
            "{}/realms/{}",
            config.keycloak.url, config.keycloak.realm
        )),
        redirect_url: Some(format!(
            "http://{}:{}",
            config.frontend.host, config.frontend.port
        )),
        registry: Some(RegistryEndpoint { url: api.clone() }),
        command_url: Some(api),
        local_certs: false,
        kafka_bootstrap_servers: Some(config.kafka.bootstrap_servers.clone()),
    }
}

pub fn kafka_config(kafka: &KafkaClientConfig, topic: &str) -> KafkaConfig {
    KafkaConfig {
        client: kafka.clone(),
        topic: topic.to_string(),
    }
}
