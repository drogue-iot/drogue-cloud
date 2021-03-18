#[inline]
pub fn enable_auth() -> bool {
    true
}

#[inline]
pub fn health_bind_addr() -> String {
    "127.0.0.1:9090".into()
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
pub fn kafka_bootstrap_servers() -> String {
    "kafka-eventing-kafka-bootstrap.knative-eventing.svc:9092".into()
}

#[inline]
pub fn oauth2_scopes() -> String {
    "openid profile email".into()
}
