use crate::utils::{Failed, JsonResponse, Succeeded};
use drogue_cloud_service_api::error::ErrorResponse;
use http::Response;
use patternfly_yew::{Toast, ToastDispatcher, Type};
use yew::format::Text;
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

impl ErrorProvider for Response<Text> {
    fn description(self) -> String {
        self.into_body().description()
    }
}

impl<T> ErrorProvider for JsonResponse<T> {
    fn description(self) -> String {
        match self.into_body().0 {
            Ok(text) => text.description(),
            Err(err) => err.description(),
        }
    }
}

impl ErrorProvider for Text {
    fn description(self) -> String {
        match self {
            Ok(t) => match serde_json::from_str::<ErrorResponse>(&t) {
                Ok(response) => response.message,
                Err(_) => t,
            },
            Err(err) => err.to_string(),
        }
    }
}

impl<E> ErrorProvider for Succeeded<yew::format::Text, E> {
    fn description(self) -> String {
        self.data.description()
    }
}

impl<E> ErrorProvider for Failed<yew::format::Text, E> {
    fn description(self) -> String {
        self.data.description()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::Response;
    use yew::format::Text;

    #[test]
    fn test_error_response() {
        let response = Response::builder()
            .status(400)
            .body(Text::Ok(
                r#"{ "error": "BadRequest", "message": "Failed to perform operation" }"#
                    .to_string(),
            ))
            .unwrap();

        let description = response.description();
        assert_eq!("Failed to perform operation", description);
    }

    #[test]
    fn test_unparsable_error() {
        let response = Response::builder()
            .status(400)
            .body(Text::Ok(r#"Failed to execute"#.to_string()))
            .unwrap();

        let description = response.description();
        assert_eq!("Failed to execute", description);
    }
}
