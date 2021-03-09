use anyhow::anyhow;
use chrono::Duration;
use drogue_cloud_service_common::openid::Expires;
use openid::Client;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let client_id = std::env::var("CLIENT_ID").unwrap_or_else(|_| "services".to_string());
    let client_secret = std::env::var("CLIENT_SECRET")?;
    let issuer_url = Url::parse(&std::env::var("ISSUER_URL")?)?;

    let client: Client<_> = openid::Client::discover(
        client_id,
        client_secret,
        None,
        issuer_url,
        // "{url}/auth/realms/{realm}/protocol/openid-connect{path}",
    )
    .await
    .map_err(|err| anyhow!("Failed to discover client: {}", err))?;

    let token = client.request_token_using_client_credentials().await?;

    println!("Token: {:?}", token);
    println!("Expires: {:?}", token.expires_in());

    println!(
        "Expires (1m): {}",
        token.expires_before(Duration::minutes(1))
    );
    println!(
        "Expires (5m): {}",
        token.expires_before(Duration::minutes(5))
    );
    println!(
        "Expires (15m): {}",
        token.expires_before(Duration::minutes(15))
    );
    println!("Expires (1h): {}", token.expires_before(Duration::hours(1)));

    Ok(())
}
