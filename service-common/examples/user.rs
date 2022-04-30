use anyhow::anyhow;
use chrono::Duration;
use drogue_client::openid::Expires;
use openid::{error::ClientError, Bearer, Client, OAuth2Error, Provider};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;
use url::{form_urlencoded::Serializer, Url};

/// Requests an access token using the Resource Owner Password Credentials Grant flow
///
/// See [RFC 6749, section 4.3](https://tools.ietf.org/html/rfc6749#section-4.3)
pub async fn username_password_request(
    client: &Client,
    username: &str,
    password: &str,
) -> Result<Bearer, ClientError> {
    // Ensure the non thread-safe `Serializer` is not kept across
    // an `await` boundary by localizing it to this inner scope.
    let body = {
        let mut body = Serializer::new(String::new());
        body.append_pair("grant_type", "password");
        body.append_pair("username", username);
        body.append_pair("password", password);
        body.append_pair("client_id", &client.client_id);
        body.append_pair("client_secret", &client.client_secret);
        body.append_pair("scope", "offline_access");
        body.finish()
    };

    let json = post_token(client, body).await?;
    let token: Bearer = serde_json::from_value(json)?;
    Ok(token)
}

async fn post_token(client: &Client, body: String) -> Result<Value, ClientError> {
    let json = client
        .http_client
        .post(client.provider.token_uri().clone())
        .basic_auth(&client.client_id, Some(&client.client_secret))
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?
        .json::<Value>()
        .await?;

    let error: Result<OAuth2Error, _> = serde_json::from_value(json.clone());

    if let Ok(error) = error {
        Err(ClientError::from(error))
    } else {
        Ok(json)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let client_id = std::env::var("CLIENT_ID").unwrap_or_else(|_| "users".to_string());
    let client_secret = std::env::var("CLIENT_SECRET")?;
    let issuer_url = Url::parse(&std::env::var("ISSUER_URL")?)?;

    let client: Client<_> = openid::Client::discover(
        client_id,
        client_secret,
        None,
        issuer_url,
        // "{url}/realms/{realm}/protocol/openid-connect{path}",
    )
    .await
    .map_err(|err| anyhow!("Failed to discover client: {}", err))?;

    let username = std::env::var("USERNAME").unwrap();
    let password = std::env::var("PASSWORD").unwrap();

    let token = username_password_request(&client, &username, &password).await?;

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

    let offline_token = client.refresh_token(token, None).await?;

    println!("Offline Token: {:?}", offline_token);
    println!("Expires: {:?}", offline_token.expires_in());

    Ok(())
}
