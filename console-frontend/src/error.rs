use crate::backend::{ApiError, ApiResponse};
use drogue_client::error::ClientError;
use patternfly_yew::{Toast, ToastDispatcher, Type};
use yew::prelude::*;

pub trait ErrorProvider {
    fn description(self) -> String;
}

pub trait ErrorNotifier {
    fn notify<S: Into<String>>(self, title: S) -> ErrorNotification;
}

impl<E: ErrorProvider> ErrorNotifier for E {
    fn notify<S: Into<String>>(self, title: S) -> ErrorNotification {
        ErrorNotification {
            title: title.into(),
            description: self.description(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ErrorNotification {
    pub title: String,
    pub description: String,
}

impl ErrorNotification {
    pub fn toast(self) {
        ToastDispatcher::default().toast(Toast {
            title: self.title,
            body: html! {<p>{self.description}</p>},
            r#type: Type::Danger,
            ..Default::default()
        });
    }
}

impl<S1: ToString, S2: ToString> From<(S1, S2)> for ErrorNotification {
    fn from(s: (S1, S2)) -> Self {
        Self {
            title: s.0.to_string(),
            description: s.1.to_string(),
        }
    }
}

pub fn error<T, E>(title: T, error: E)
where
    T: ToString,
    E: ErrorProvider,
{
    ErrorNotification::from((title.to_string(), error.description())).toast();
}

impl ErrorProvider for &str {
    fn description(self) -> String {
        self.to_string()
    }
}

impl ErrorProvider for String {
    fn description(self) -> String {
        self
    }
}

impl ErrorProvider for anyhow::Error {
    fn description(self) -> String {
        self.to_string()
    }
}

impl ErrorProvider for ClientError {
    fn description(self) -> String {
        self.to_string()
    }
}

impl ErrorProvider for ApiError {
    fn description(self) -> String {
        match self {
            ApiError::Response(response, _) => response.message,
            ApiError::Internal(err) => format!("Internal error: {}", err),
            ApiError::Unknown(_, code) => format!("Unknown response (code: {})", code),
        }
    }
}

/// Error provider for an API response
impl<T> ErrorProvider for ApiResponse<T> {
    fn description(self) -> String {
        match self {
            // if we need to provide an error for a successful API response, then obviously the code wasn't correct
            Self::Success(_, code) => format!("Invalid response code: {}", code),
            // standard error
            Self::Failure(err) => err.description(),
        }
    }
}
