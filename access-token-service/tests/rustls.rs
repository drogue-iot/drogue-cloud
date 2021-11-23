use drogue_cloud_service_common::reqwest::make_insecure;

/// Working with [`Any`], we need to ensure that types match, especially when updating versions.
#[tokio::test]
pub async fn test_rustls_insecure() {
    let mut builder = reqwest::ClientBuilder::new();
    builder = make_insecure(builder);
    let _ = builder.build().unwrap();
}
