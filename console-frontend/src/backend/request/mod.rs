mod api;
mod handler;
mod json;
mod payload;

pub use api::*;
pub use handler::*;
pub use json::*;
pub use payload::*;

use anyhow::anyhow;
use drogue_cloud_service_api::error::ErrorResponse;
use http::{Method, StatusCode};
use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, Cow};
use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, Headers, RequestCache, RequestCredentials, RequestInit, RequestMode,
    RequestRedirect, Window,
};
use yew::{html::Scope, Component, Context};

pub struct RequestBuilder<'b> {
    method: Method,
    /// URL of the request
    ///
    /// *Note:* This may also be a relative URL.
    url: String,
    query: Vec<(Cow<'b, str>, Cow<'b, str>)>,
    headers: Vec<(Cow<'b, str>, Cow<'b, str>)>,

    body: Option<JsValue>,
    content_type: Option<String>,

    cache: Option<RequestCache>,
    mode: Option<RequestMode>,
    redirect: Option<RequestRedirect>,
    credentials: Option<RequestCredentials>,
}

impl<'b> RequestBuilder<'b> {
    pub fn new<U: Into<String>>(method: Method, url: U) -> Self {
        Self {
            method,
            url: url.into(),
            headers: vec![],
            query: vec![],

            body: None,
            content_type: None,

            cache: None,
            mode: None,
            redirect: None,
            credentials: None,
        }
    }

    pub fn query(mut self, key: Cow<'b, str>, value: Cow<'b, str>) -> Self {
        self.query.push((key, value));
        self
    }

    pub fn header(mut self, key: Cow<'b, str>, value: Cow<'b, str>) -> Self {
        self.headers.push((key, value));
        self
    }

    pub fn cache<T: Into<Option<RequestCache>>>(mut self, cache: T) -> Self {
        self.cache = cache.into();
        self
    }

    pub fn mode<T: Into<Option<RequestMode>>>(mut self, mode: T) -> Self {
        self.mode = mode.into();
        self
    }

    pub fn redirect<T: Into<Option<RequestRedirect>>>(mut self, redirect: T) -> Self {
        self.redirect = redirect.into();
        self
    }

    #[allow(dead_code)]
    pub fn credentials<T: Into<Option<RequestCredentials>>>(
        mut self,
        credentials: RequestCredentials,
    ) -> Self {
        self.credentials = credentials.into();
        self
    }

    pub fn body<P>(mut self, body: P) -> Result<Self, P::Error>
    where
        P: RequestPayload,
    {
        self.content_type = body.content_type();
        let body = body.into_js()?;
        self.body = body;
        Ok(self)
    }

    pub fn send<H>(self, handler: H) -> RequestHandle
    where
        H: RequestHandler<anyhow::Result<Response>>,
    {
        Request::new(self).start(handler)
    }
}

pub struct Request {
    url: String,
    init: RequestInit,
    window: Window,
    abort_controller: Option<AbortController>,
}

impl Request {
    pub fn new(request: RequestBuilder) -> Self {
        let abort_controller = AbortController::new().ok();

        let mut init = RequestInit::new();

        init.method(request.method.as_str());

        if let Some(cache) = request.cache {
            init.cache(cache);
        }
        if let Some(credentials) = request.credentials {
            init.credentials(credentials);
        }
        if let Some(redirect) = request.redirect {
            init.redirect(redirect);
        }
        if let Some(mode) = request.mode {
            init.mode(mode);
        }

        if let Ok(headers) = Headers::new() {
            if let Some(content_type) = request.content_type {
                headers.append("Content-Type", &content_type).ok();
            }

            for (k, v) in request.headers {
                headers.append(k.borrow(), v.borrow()).ok();
            }
            init.headers(&headers);
        }

        if let Some(abort_controller) = &abort_controller {
            init.signal(Some(&abort_controller.signal()));
        }

        let query = if !request.query.is_empty() {
            let mut query = Vec::<String>::new();
            for (k, v) in request.query {
                query.push(format!("{}={}", k, v));
            }
            Some(query.join("&"))
        } else {
            None
        };

        init.body(request.body.as_ref());

        let window = gloo_utils::window();
        let url = if let Some(query) = query {
            format!("{}?{}", request.url.to_string(), query)
        } else {
            request.url.to_string()
        };
        Self {
            url,
            init,
            window,
            abort_controller,
        }
    }

    pub fn start<H>(mut self, handler: H) -> RequestHandle
    where
        H: RequestHandler<anyhow::Result<Response>>,
    {
        let active = Rc::new(AtomicBool::new(true));
        let abort_controller = self.abort_controller.take();
        let handle = RequestHandle {
            active,
            abort_controller,
        };
        handler.execute(handle.make_context(), self.execute());
        handle
    }

    async fn execute(self) -> anyhow::Result<Response> {
        self.execute_js().await.map_err(js_err)
    }

    async fn execute_js(self) -> Result<Response, JsValue> {
        let request = web_sys::Request::new_with_str_and_init(&self.url, &self.init)?;
        let response = JsFuture::from(self.window.fetch_with_request(&request)).await?;
        let response: web_sys::Response = response.dyn_into()?;

        let data = JsFuture::from(response.array_buffer()?).await?;
        let data = Uint8Array::new(&data).to_vec();

        Ok(Response { response, data })
    }
}

pub struct Response {
    pub response: web_sys::Response,
    pub data: Vec<u8>,
}

#[must_use = "The operation will be aborted when the handle is dropped"]
pub struct RequestHandle {
    active: Rc<AtomicBool>,
    abort_controller: Option<AbortController>,
}

impl RequestHandle {
    fn was_active(&self) -> bool {
        self.active.swap(false, Ordering::SeqCst)
    }

    pub(crate) fn make_context(&self) -> RequestContext {
        RequestContext {
            active: self.active.clone(),
        }
    }
}

pub struct RequestContext {
    active: Rc<AtomicBool>,
}

impl RequestContext {
    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

impl Drop for RequestHandle {
    fn drop(&mut self) {
        if self.was_active() {
            if let Some(abort_controller) = &self.abort_controller {
                abort_controller.abort();
            }
        }
    }
}

pub(crate) fn js_err(err: JsValue) -> anyhow::Error {
    if let Some(err) = err.as_string() {
        anyhow!("Request failed: {}", err)
    } else {
        anyhow!("Request failed: <unknown>")
    }
}
