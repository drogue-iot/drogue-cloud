use drogue_cloud_service_api::PROJECT;
use drogue_cloud_service_common::{keycloak::client::KeycloakAdminClient, runtime};
use drogue_cloud_user_auth_service::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    runtime!(PROJECT).exec(run::<KeycloakAdminClient>).await
}
