use actix_web::{
    dev::{Payload, PayloadStream},
    FromRequest, HttpRequest,
};
use futures_util::future::{ready, Ready};

#[derive(Clone, Debug)]
pub struct ClientCertificateChain(pub Vec<Vec<u8>>);

impl FromRequest for ClientCertificateChain {
    type Error = ();
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _payload: &mut Payload<PayloadStream>) -> Self::Future {
        let result = req.extensions().get::<ClientCertificateChain>().cloned();

        ready(result.ok_or(()))
    }
}
