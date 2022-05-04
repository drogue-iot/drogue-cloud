use url::Url;

#[inline]
pub fn enable_access_token() -> bool {
    true
}

#[inline]
pub fn realm() -> String {
    "drogue".into()
}

#[inline]
pub fn health_bind_addr() -> String {
    "127.0.0.1:9090".into()
}

#[inline]
pub fn health_workers() -> usize {
    1
}

#[inline]
pub fn max_payload_size() -> usize {
    65536
}

#[inline]
pub fn max_json_payload_size() -> usize {
    65536
}

#[inline]
pub fn bind_addr() -> String {
    "127.0.0.1:8080".into()
}

#[inline]
pub fn kafka_command_topic() -> String {
    "iot-commands".into()
}

#[inline]
pub fn oauth2_scopes() -> String {
    "openid profile email".into()
}

#[inline]
pub fn enable_kube() -> bool {
    true
}

#[inline]
pub fn check_kafka_topic_ready() -> bool {
    true
}

#[inline]
pub fn authentication_url() -> Url {
    Url::parse("http://authentication-service").unwrap()
}

#[inline]
pub fn user_auth_url() -> Url {
    Url::parse("http://user-auth-service").unwrap()
}

#[inline]
pub fn registry_url() -> Url {
    Url::parse("http://device-management-service").unwrap()
}

#[inline]
pub fn keycloak_url() -> Url {
    Url::parse("https://keycloak:8443").unwrap()
}

#[inline]
pub fn device_state_url() -> Url {
    Url::parse("http://device-state-service").unwrap()
}

#[inline]
pub fn mqtts_port() -> u16 {
    8883
}

#[inline]
pub fn instance() -> String {
    "drogue".into()
}
