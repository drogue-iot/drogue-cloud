use chrono::Duration;
use openid::{
    error::Error,
    validation::{validate_token_aud, validate_token_exp, validate_token_issuer},
    Claims, Client, CompactJson, Configurable, IdToken, Provider,
};

/// This is "fork" of the original [`Client::validate_token`] function. However, we are not
/// validating the nonce here, as this would be done in the client (browser, CLI) and not in the
/// backend.
pub fn validate_token<C: CompactJson + Claims, P: Provider + Configurable>(
    client: &Client<P, C>,
    token: &IdToken<C>,
    max_age: Option<&Duration>,
) -> Result<(), Error> {
    let claims = token.payload()?;
    let config = client.config();

    validate_token_issuer(claims, config)?;
    validate_token_aud(claims, &client.client_id)?;
    validate_token_exp(claims, max_age)?;

    Ok(())
}
