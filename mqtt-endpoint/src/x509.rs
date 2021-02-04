use rust_tls::Session;

drogue_cloud_endpoint_common::retriever!();

#[cfg(feature = "rustls")]
drogue_cloud_endpoint_common::retriever_rustls!(ntex::server::rustls::TlsStream<T>);

#[cfg(feature = "openssl")]
drogue_cloud_endpoint_common::retriever_openssl!(ntex::server::openssl::SslStream<T>);

drogue_cloud_endpoint_common::retriever_none!(ntex::rt::net::TcpStream);
