pub fn global_sso() -> Option<String> {
    // try fetching global SSO url
    std::env::var("SSO_URL").ok()
}
