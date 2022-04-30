pub(crate) fn sso_to_issuer_url(sso: &str, realm: &str) -> String {
    format!("{}/realms/{}", sso, realm)
}
