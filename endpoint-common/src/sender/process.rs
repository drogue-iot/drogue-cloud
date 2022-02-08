use cloudevents::event::ExtensionValue;
use cloudevents::{AttributesReader, AttributesWriter};
use drogue_client::registry::v1::{Step, When};
use drogue_client::{
    registry::{
        self,
        v1::{Application, PublishSpec},
    },
    Translator,
};
use reqwest::Url;
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
    #[error("Build event error")]
    Build(#[from] cloudevents::event::EventBuilderError),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Internal error: {0}")]
    Internal(Box<dyn std::error::Error + Send>),
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

pub struct Processor(PublishSpec);

impl Processor {
    #[inline]
    pub fn new(spec: PublishSpec) -> Self {
        Self(spec)
    }

    #[instrument(skip_all, err, fields(num_rules=self.0.rules.len()))]
    pub async fn process(&self, mut event: cloudevents::Event) -> Result<Outcome, Error> {
        for rule in &self.0.rules {
            if Self::is_when(&rule.when, &event) {
                event = match Self::handle(&rule.then, event).await? {
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

    async fn handle(then: &[Step], mut event: cloudevents::Event) -> Result<StepOutcome, Error> {
        for step in then {
            event = match Self::step(step, event).await? {
                StepOutcome::Continue(event) => event,
                StepOutcome::Accept(event) => return Ok(StepOutcome::Accept(event)),
                StepOutcome::Drop => return Ok(StepOutcome::Drop),
                StepOutcome::Reject(reason) => return Ok(StepOutcome::Reject(reason)),
            }
        }

        Ok(StepOutcome::Continue(event))
    }

    async fn step(step: &Step, mut event: cloudevents::Event) -> Result<StepOutcome, Error> {
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

impl TryFrom<&registry::v1::Application> for Processor {
    type Error = serde_json::Error;

    fn try_from(value: &Application) -> Result<Self, Self::Error> {
        Ok(Self::new(
            value
                .section::<registry::v1::PublishSpec>()
                .transpose()?
                .unwrap_or_default(),
        ))
    }
}

impl TryFrom<serde_json::Value> for Processor {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(Self::new(serde_json::from_value(value)?))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cloudevents::EventBuilder;
    use serde_json::json;

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
        let processor = Processor::try_from(json!({
            "rules": [
                {
                    "when": {
                        "isChannel": "chan1",
                    },
                    "then": [
                        { "setAttribute": { "name": "dataschema", "value": "urn:my:schema" } },
                        { "removeExtension": "my-ext-1" },
                    ]
                }
            ],
        }))
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

    async fn assert_process(spec: serde_json::Value, input: cloudevents::Event, expected: Outcome) {
        let processor = Processor::try_from(spec).unwrap();
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
