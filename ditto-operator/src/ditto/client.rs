use crate::ditto::DevopsCommand;
use url::{ParseError, Url};

#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
    url: Url,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to build URL")]
    Url(#[from] ParseError),
    #[error("failed to execute request")]
    Request(#[from] reqwest::Error),
}

impl Client {
    pub fn new(
        client: reqwest::Client,
        url: Url,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        Self {
            client,
            url,
            username,
            password,
        }
    }

    pub async fn devops(&self, command: DevopsCommand) -> Result<(), Error> {
        let url = self.url.join("/devops/piggyback/connectivity")?;

        let req = self.client.post(url);

        let req = if let Some(username) = &self.username {
            req.basic_auth(username, self.password.as_ref())
        } else {
            req
        };

        let req = req.json(&command);

        req.send().await?.error_for_status()?;

        Ok(())
    }
}
