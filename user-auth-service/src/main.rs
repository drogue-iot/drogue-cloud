use drogue_cloud_service_common::{keycloak::client::KeycloakAdminClient, main};
use drogue_cloud_user_auth_service::{run, Config};

#[drogue_cloud_service_api::webapp::main]
async fn main() -> anyhow::Result<()> {
    main!(run::<KeycloakAdminClient>(Config::from_env()?).await)
}
