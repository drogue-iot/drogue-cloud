mod external;

pub use external::{ExternalClientPool, ExternalClientPoolConfig};

use crate::sender::{
    is_json,
    process::external::{ExternalError, IntoPayload},
    Direction,
};
use cloudevents::{event::ExtensionValue, AttributesReader, AttributesWriter};
use drogue_client::{
    registry::{
        self,
        v1::{Application, EnrichSpec, ResponseType, Rule, Step, ValidateSpec, When},
    },
    Translator,
};
use http::{header::CONTENT_TYPE, StatusCode};
use reqwest::Url;
use serde_json::Value;
use thiserror::Error;
use tracing::instrument;

pub enum StepOutcome {
    Continue(cloudevents::Event),
    Accept(cloudevents::Event),
    Reject(String),
    Drop,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Build event error: {0}")]
    Build(#[from] cloudevents::event::EventBuilderError),
    #[error("Event error: {0}")]
    Event(#[from] cloudevents::message::Error),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Internal error: {0}")]
    Internal(Box<dyn std::error::Error + Send>),
    #[error("External endpoint error: {0}")]
    ExternalEndpoint(#[from] ExternalError),
    #[error("External endpoint response: {0}")]
    ExternalResponse(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    // Accept event
    Accepted(cloudevents::Event),
    // Reject with a reason
    Rejected(String),
    // Silently drop (reports accepted)
    Dropped,
}

pub struct Processor {
    pool: ExternalClientPool,
    rules: Vec<Rule>,
}

impl Processor {
    #[inline]
    pub fn new(pool: ExternalClientPool, rules: Vec<Rule>) -> Self {
        Self { pool, rules }
    }

    #[instrument(level = "debug", skip_all, err, fields(num_rules=self.rules.len()))]
    pub async fn process(&self, mut event: cloudevents::Event) -> Result<Outcome, Error> {
        for rule in &self.rules {
            if Self::is_when(&rule.when, &event) {
                event = match self.handle(&rule.then, event).await? {
                    // continue processing
                    StepOutcome::Continue(event) => event,
                    // stop processing, return as accepted
                    StepOutcome::Accept(event) => return Ok(Outcome::Accepted(event)),
                    // stop processing, return as dropped
                    StepOutcome::Drop => return Ok(Outcome::Dropped),
                    // stop processing, return as rejected
                    StepOutcome::Reject(reason) => return Ok(Outcome::Rejected(reason)),
                };
            }
        }

        // all rules processed, no one rejected or dropped, so accept
        Ok(Outcome::Accepted(event))
    }

    fn is_when(when: &When, event: &cloudevents::Event) -> bool {
        match when {
            // matches always
            When::Always => true,
            // invert outcome
            When::Not(when) => Self::is_when(when, event),
            // matches when not empty and all children match
            When::And(when) => {
                if when.is_empty() {
                    return false;
                }
                for when in when {
                    if !Self::is_when(when, event) {
                        return false;
                    }
                }
                true
            }
            // matches when not empty and any children matches
            When::Or(when) => {
                let mut result = false;
                for when in when {
                    if Self::is_when(when, event) {
                        result = true;
                        break;
                    }
                }
                result
            }
            // matches when the matches is equal
            When::IsChannel(channel) => match event.subject() {
                Some(subject) => channel == subject,
                _ => false,
            },
        }
    }

    async fn handle(
        &self,
        then: &[Step],
        mut event: cloudevents::Event,
    ) -> Result<StepOutcome, Error> {
        for step in then {
            event = match self.step(step, event).await? {
                StepOutcome::Continue(event) => event,
                StepOutcome::Accept(event) => return Ok(StepOutcome::Accept(event)),
                StepOutcome::Drop => return Ok(StepOutcome::Drop),
                StepOutcome::Reject(reason) => return Ok(StepOutcome::Reject(reason)),
            }
        }

        Ok(StepOutcome::Continue(event))
    }

    async fn step(&self, step: &Step, mut event: cloudevents::Event) -> Result<StepOutcome, Error> {
        match step {
            Step::Drop => Ok(StepOutcome::Drop),
            Step::Break => Ok(StepOutcome::Accept(event)),
            Step::Reject(reason) => Ok(StepOutcome::Reject(reason.to_owned())),
            Step::SetAttribute { name, value } => Ok(StepOutcome::Continue(Self::set_attribute(
                event,
                name,
                Some(value),
            )?)),
            Step::RemoveAttribute(name) => Ok(StepOutcome::Continue(Self::set_attribute(
                event, name, None,
            )?)),
            Step::SetExtension { name, value } => {
                event.set_extension(name, ExtensionValue::String(value.to_string()));
                Ok(StepOutcome::Continue(event))
            }
            Step::RemoveExtension(name) => {
                event.remove_extension(name);
                Ok(StepOutcome::Continue(event))
            }
            Step::Validate(spec) => self.validate(spec, event).await,
            Step::Enrich(spec) => self.enrich(spec, event).await,
        }
    }

    #[instrument(skip_all, fields(
        request_type=?spec.request,
        response_type=?spec.response,
    ))]
    async fn enrich(
        &self,
        spec: &EnrichSpec,
        event: cloudevents::Event,
    ) -> Result<StepOutcome, Error> {
        let client = self.pool.get(&spec.endpoint).await?;

        log::debug!("Expected response type: {:?}", spec.response);

        match spec.response {
            ResponseType::CloudEvent | ResponseType::AssumeStructuredCloudEvent => {
                let response = client
                    .process(spec.request.to_payload(event), &spec.endpoint)
                    .await?;
                log::debug!("External endpoint reported: {}", response.status());

                match response.status() {
                    StatusCode::OK => {
                        let event =
                            if matches!(spec.response, ResponseType::AssumeStructuredCloudEvent) {
                                // we assume it is a structured cloud event, and just deserialize it
                                response.json().await.map_err(ExternalError::Request)?
                            } else {
                                // we do the proper processing, handling binary and structured mode
                                cloudevents::binding::reqwest::response_to_event(response).await?
                            };

                        Ok(StepOutcome::Continue(event))
                    }
                    code => Err(Error::ExternalResponse(format!(
                        "Unexpected endpoint response: {code}"
                    ))),
                }
            }
            ResponseType::Raw => {
                let response = client
                    .process(spec.request.to_payload(event.clone()), &spec.endpoint)
                    .await?;
                log::debug!("External endpoint reported: {}", response.status());

                match response.status() {
                    StatusCode::OK => Ok(StepOutcome::Continue({
                        let mut event = event;
                        let content_type = response
                            .headers()
                            .get(http::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        let data = response
                            .bytes()
                            .await
                            .map_err(ExternalError::Request)?
                            .to_vec();
                        event.set_data(content_type, data);
                        event
                    })),
                    code => Err(Error::ExternalResponse(format!(
                        "Unexpected endpoint response: {code}"
                    ))),
                }
            }
        }
    }

    #[instrument(skip_all, fields(request_type=?spec.request))]
    async fn validate(
        &self,
        spec: &ValidateSpec,
        event: cloudevents::Event,
    ) -> Result<StepOutcome, Error> {
        let client = self.pool.get(&spec.endpoint).await?;
        let response = client
            .process(spec.request.to_payload(event.clone()), &spec.endpoint)
            .await?;

        log::debug!("External endpoint reported: {}", response.status());

        match response.status() {
            // event is ok, progress
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(StepOutcome::Continue(event)),
            // event is accepted directly
            StatusCode::ACCEPTED => Ok(StepOutcome::Accept(event)),
            // client error -> reject, extract reason
            code if code.is_client_error() => {
                let reason = if is_json(
                    response
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or(""),
                ) {
                    let body: Value = response.json().await.map_err(ExternalError::Request)?;
                    body["reason"].as_str().unwrap_or("Rejected").into()
                } else {
                    response
                        .text()
                        .await
                        .map_err(ExternalError::Request)?
                        .clone()
                };
                Ok(StepOutcome::Reject(reason))
            }
            // just fail
            code => Err(Error::ExternalResponse(format!(
                "Unexpected endpoint response: {code}"
            ))),
        }
    }

    fn set_attribute(
        mut event: cloudevents::Event,
        name: &str,
        value: Option<&str>,
    ) -> Result<cloudevents::Event, Error> {
        match name {
            "datacontenttype" => {
                event.set_datacontenttype(value.map(|s| s.to_string()));
                Ok(event)
            }
            "dataschema" => {
                let url = value
                    .map(Url::parse)
                    .transpose()
                    .map_err(|err| Error::Config(format!("Invalid URL: {}", err)))?;
                event.set_dataschema(url);
                Ok(event)
            }
            "subject" => {
                event.set_subject(value);
                Ok(event)
            }
            "type" => {
                if let Some(value) = value {
                    event.set_type(value);
                    Ok(event)
                } else {
                    Err(Error::Config(
                        "Removing the 'type' attribute is not valid".to_string(),
                    ))
                }
            }
            name => Err(Error::Config(format!(
                "Unknown or immutable attribute: {}",
                name
            ))),
        }
    }
}

impl TryFrom<(Direction, &registry::v1::Application, ExternalClientPool)> for Processor {
    type Error = serde_json::Error;

    fn try_from(value: (Direction, &Application, ExternalClientPool)) -> Result<Self, Self::Error> {
        Ok(Self::new(
            value.2,
            match value.0 {
                Direction::Upstream => value
                    .1
                    .section::<registry::v1::CommandSpec>()
                    .transpose()?
                    .map(|spec| spec.rules),
                Direction::Downstream => value
                    .1
                    .section::<registry::v1::PublishSpec>()
                    .transpose()?
                    .map(|spec| spec.rules),
            }
            .unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cloudevents::EventBuilder;
    use drogue_client::registry::v1::PublishSpec;
    use serde_json::json;

    impl TryFrom<serde_json::Value> for Processor {
        type Error = serde_json::Error;

        fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
            Ok(Self::new(
                Default::default(),
                serde_json::from_value(value)?,
            ))
        }
    }

    impl From<Vec<Rule>> for Processor {
        fn from(rules: Vec<Rule>) -> Self {
            Self::new(Default::default(), rules)
        }
    }

    #[tokio::test]
    async fn test_default() {
        assert_process(
            json!({}),
            event("id1", "type", "source", "chan1").build().unwrap(),
            Outcome::Accepted(event("id1", "type", "source", "chan1").build().unwrap()),
        )
        .await
    }

    #[tokio::test]
    async fn test_s1() {
        let processor = Processor::try_from(json!(
             [
                {
                    "when": {
                        "isChannel": "chan1",
                    },
                    "then": [
                        { "setAttribute": { "name": "dataschema", "value": "urn:my:schema" } },
                        { "removeExtension": "my-ext-1" },
                    ]
                }
            ]
        ))
        .unwrap();

        assert_eq!(
            processor
                .process(
                    event("id1", "type", "source", "chan1")
                        .extension("my-ext-1", "value1")
                        .data("application/json", json!({}))
                        .build()
                        .unwrap()
                )
                .await
                .unwrap(),
            Outcome::Accepted(
                event("id1", "type", "source", "chan1")
                    .data_with_schema("application/json", "urn:my:schema", json!({}))
                    .build()
                    .unwrap(),
            )
        )
    }

    #[tokio::test]
    async fn test_parse_1() {
        let spec = json!({
          "rules": [
            {
              "when":
                {
                    "isChannel": "state"
                },
              "then": [
                {
                  "enrich": {
                    "response": "raw",
                    "endpoint":{
                        "method": "POST",
                        "url": "https://some-external-service/path/to"
                    }
                  }
                }
              ]
            }
          ]
        });
        let spec: PublishSpec = serde_json::from_value(spec).unwrap();
        assert!(matches!(
            spec.rules[0].then[0],
            Step::Enrich(EnrichSpec {
                response: ResponseType::Raw,
                ..
            })
        ));
    }

    async fn assert_process(spec: serde_json::Value, input: cloudevents::Event, expected: Outcome) {
        let spec: PublishSpec = serde_json::from_value(spec).unwrap();
        let processor = Processor::from(spec.rules);
        let output = processor.process(input.clone()).await.unwrap();

        assert_eq!(output, expected);
    }

    fn event<S1, S2, S3, S4>(
        id: S1,
        ty: S2,
        source: S3,
        subject: S4,
    ) -> cloudevents::EventBuilderV10
    where
        S1: Into<String>,
        S2: Into<String>,
        S3: Into<String>,
        S4: Into<String>,
    {
        cloudevents::EventBuilderV10::new()
            .id(id)
            .ty(ty)
            .source(source)
            .subject(subject)
    }
}
