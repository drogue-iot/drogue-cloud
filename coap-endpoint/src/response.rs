use crate::error::CoapEndpointError;
use coap_lite::{CoapRequest, CoapResponse};
use std::net::SocketAddr;

pub trait Responder {
    fn respond_to(self, req: &mut CoapRequest<SocketAddr>) -> Option<CoapResponse>;
}

impl Responder for Result<Option<CoapResponse>, CoapEndpointError> {
    fn respond_to(self, req: &mut CoapRequest<SocketAddr>) -> Option<CoapResponse> {
        match self {
            Ok(val) => val,
            Err(e) => {
                req.response.as_mut().and_then(|v| {
                    v.set_status(e.status_code());
                    v.message.payload = e.to_string()[..].as_bytes().to_vec();
                    Some(v.clone())
                });
                req.response.clone()
            }
        }
    }
}
