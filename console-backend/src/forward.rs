use actix_web::{web, Error, HttpRequest, HttpResponse};
use awc::Client;
use url::Url;

pub async fn forward(
    req: HttpRequest,
    body: web::Bytes,
    url: web::Data<Url>,
    client: web::Data<Client>,
) -> Result<HttpResponse, Error> {
    let mut new_url = url.get_ref().clone();
    new_url.set_path(req.uri().path());
    new_url.set_query(req.uri().query());

    // TODO: This forwarded implementation is incomplete as it only handles the inofficial
    // X-Forwarded-For header but not the official Forwarded one.
    let forwarded_req = client
        .request_from(new_url.as_str(), req.head())
        .no_decompress();
    let mut forwarded_req = if let Some(addr) = req.head().peer_addr {
        forwarded_req.append_header(("x-forwarded-for", format!("{}", addr.ip())))
    } else {
        forwarded_req
    };

    log::info!("Headers: {:#?}", forwarded_req.headers());

    // rewrite host
    let forwarded_req = if let Some(host) = url.get_ref().host_str() {
        forwarded_req.headers_mut().remove("host");
        forwarded_req.append_header((
            "host",
            format!(
                "{}{}",
                host,
                url.get_ref()
                    .port()
                    .map(|p| format!(":{}", p))
                    .unwrap_or_default()
            ),
        ))
    } else {
        forwarded_req
    };

    log::info!("Headers (post): {:#?}", forwarded_req.headers());

    let mut res = forwarded_req.send_body(body).await.map_err(Error::from)?;

    let mut client_resp = HttpResponse::build(res.status());
    // Remove `Connection` as per
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
    for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
        client_resp.append_header((header_name.clone(), header_value.clone()));
    }

    Ok(client_resp.body(res.body().await?))
}
