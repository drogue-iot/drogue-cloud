use cached::proc_macro::cached;
use drogue_cloud_service_common::error::ServiceError;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::ListParams;
use kube::Api;

#[cached(time = 300, result = true, key = "bool", convert = r#"{ true }"#)]
pub async fn get_demos(
    config_maps: &Api<ConfigMap>,
) -> Result<Vec<(String, String)>, ServiceError> {
    let mut result = vec![];

    for cm in config_maps
        .list(&ListParams::default().labels("demo"))
        .await
        .map_err(|_| ServiceError::ServiceUnavailable("Failed to enumerate demos".into()))?
    {
        if let (Some(label), Some(href)) = (cm.data.get("label"), cm.data.get("href")) {
            result.push((label.to_string(), href.to_string()));
        }
    }

    Ok(result)
}
