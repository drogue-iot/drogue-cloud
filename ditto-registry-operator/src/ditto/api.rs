//! The HTTP API

use super::data::*;
use crate::ditto::Error;
use http::Method;
use serde_json::Value;

pub trait RequestFactory {
    fn new_request<S: AsRef<str>>(
        &self,
        method: Method,
        path: S,
    ) -> Result<reqwest::RequestBuilder, Error>;
}

pub trait Request {
    type Response;

    fn into_builder<F: RequestFactory>(self, factory: &F)
        -> Result<reqwest::RequestBuilder, Error>;
}

#[allow(unused)]
pub enum PolicyOperation {
    CreateOrUpdate(Policy),
    Delete(EntityId),
}

#[allow(unused)]
pub enum ThingOperation {
    CreateOrUpdate(Thing),
    Delete(EntityId),
}

impl Request for PolicyOperation {
    type Response = Value;

    fn into_builder<F: RequestFactory>(
        self,
        factory: &F,
    ) -> Result<reqwest::RequestBuilder, Error> {
        Ok(match self {
            Self::CreateOrUpdate(policy) => factory
                .new_request(
                    Method::PUT,
                    format!("policies/{policyId}", policyId = policy.policy_id),
                )?
                .json(&policy),
            Self::Delete(policy_id) => factory.new_request(
                Method::DELETE,
                format!("policies/{policyId}", policyId = policy_id),
            )?,
        })
    }
}

impl Request for ThingOperation {
    type Response = Value;

    fn into_builder<F: RequestFactory>(
        self,
        factory: &F,
    ) -> Result<reqwest::RequestBuilder, Error> {
        Ok(match self {
            Self::CreateOrUpdate(thing) => factory
                .new_request(
                    Method::PUT,
                    format!("things/{thingId}", thingId = thing.thing_id),
                )?
                .json(&thing),
            Self::Delete(thing_id) => factory.new_request(
                Method::DELETE,
                format!("things/{thingId}", thingId = thing_id),
            )?,
        })
    }
}
