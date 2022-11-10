use clap::ArgMatches;
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
    pub command_routing: Endpoint,
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
            .get_one::<String>("bind-address")
            .map(|s| s.as_str())
            .unwrap_or("localhost")
            .to_string();

        let with_tls = matches.get_one::<String>("server-cert").is_some()
            && matches.get_one::<String>("server-key").is_some();

        ServerConfig {
            tls_insecure: matches.get_flag("insecure"),
            tls_ca_certificates: vec![],
            kafka: KafkaClientConfig {
                bootstrap_servers: matches
                    .get_one::<String>("kafka-bootstrap-servers")
                    .map(|s| s.as_str())
                    .unwrap_or("localhost:9092")
                    .to_string(),
                properties: HashMap::new(),
            },
            database: Database {
                endpoint: Endpoint {
                    host: matches
                        .get_one::<String>("database-host")
                        .map(|s| s.as_str())
                        .unwrap_or("localhost")
                        .to_string(),
                    port: matches
                        .get_one::<u16>("database-port")
                        .copied()
                        .unwrap_or(5432u16),
                },
                db: matches
                    .get_one::<String>("database-name")
                    .map(|s| s.as_str())
                    .unwrap_or("drogue")
                    .to_string(),
                user: matches
                    .get_one::<String>("database-user")
                    .map(|s| s.as_str())
                    .unwrap_or("admin")
                    .to_string(),
                password: matches
                    .get_one::<String>("database-password")
                    .map(|s| s.as_str())
                    .unwrap_or("admin123456")
                    .to_string(),
            },
            keycloak: Keycloak {
                url: matches
                    .get_one::<String>("keycloak-url")
                    .map(|s| s.as_str())
                    .unwrap_or("http://localhost:8081")
                    .to_string(),
                realm: matches
                    .get_one::<String>("keycloak-realm")
                    .map(|s| s.as_str())
                    .unwrap_or("drogue")
                    .to_string(),
                user: matches
                    .get_one::<String>("keycloak-user")
                    .map(|s| s.as_str())
                    .unwrap_or("admin")
                    .to_string(),
                password: matches
                    .get_one::<String>("keycloak-password")
                    .map(|s| s.as_str())
                    .unwrap_or("admin123456")
                    .to_string(),
            },
            drogue: Drogue {
                admin_user: matches
                    .get_one::<String>("drogue-admin-user")
                    .map(|s| s.as_str())
                    .unwrap_or("admin")
                    .to_string(),
                admin_password: matches
                    .get_one::<String>("drogue-admin-password")
                    .map(|s| s.as_str())
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
                port: if with_tls { 8883 } else { 1883 },
            },
            mqtt_ws: Endpoint {
                host: iface.to_string(),
                port: if with_tls { 20443 } else { 20880 },
            },
            mqtt_ws_browser: Endpoint {
                host: iface.to_string(),
                port: if with_tls { 21443 } else { 21880 },
            },
            http: Endpoint {
                host: iface.to_string(),
                port: 8088,
            },
            coap: Endpoint {
                host: iface.to_string(),
                port: if with_tls { 5684 } else { 5683 },
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
                host: iface.to_string(),
                port: 10007,
            },
            command_routing: Endpoint {
                host: iface,
                port: 10008,
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
