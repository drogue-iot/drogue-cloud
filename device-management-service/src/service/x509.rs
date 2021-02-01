use chrono::{TimeZone, Utc};
use drogue_cloud_database_common::{error::ServiceError, models::TypedAlias};
use drogue_cloud_service_api::management::{
    ApplicationSpecTrustAnchors, ApplicationStatusTrustAnchorEntry, ApplicationStatusTrustAnchors,
};
use std::collections::HashSet;
use x509_parser::parse_x509_certificate;

pub fn process_anchors(
    spec: ApplicationSpecTrustAnchors,
) -> Result<(ApplicationStatusTrustAnchors, HashSet<TypedAlias>), ServiceError> {
    let mut anchors = Vec::with_capacity(spec.anchors.len());
    let mut aliases = HashSet::new();

    for anchor in spec.anchors {
        let a = match process_anchor(&anchor.certificate) {
            Ok(ta) => {
                if let ApplicationStatusTrustAnchorEntry::Valid { subject, .. } = &ta {
                    aliases.insert(TypedAlias("x509/ca".into(), subject.clone()));
                }
                ta
            }
            Err(message) => ApplicationStatusTrustAnchorEntry::Invalid {
                error: "Failed".into(),
                message,
            },
        };
        log::debug!("Anchor processed: {:?}", a);
        anchors.push(a);
    }

    Ok((ApplicationStatusTrustAnchors { anchors }, aliases))
}

fn process_anchor(certs: &[u8]) -> Result<ApplicationStatusTrustAnchorEntry, String> {
    let pems = pem::parse_many(&certs);

    for pem in pems {
        if pem.tag == "CERTIFICATE" {
            let cert = parse_x509_certificate(&pem.contents)
                .map_err(|err| format!("Failed to parse certificate: {}", err))?
                .1;

            let not_before = Utc.timestamp(cert.tbs_certificate.validity.not_before.timestamp(), 0);
            let not_after = Utc.timestamp(cert.tbs_certificate.validity.not_after.timestamp(), 0);

            return Ok(ApplicationStatusTrustAnchorEntry::Valid {
                subject: cert.tbs_certificate.subject.to_string(),
                certificate: certs.into(),
                not_before,
                not_after,
            });
        }
    }

    // Failed to find a certificate

    Ok(ApplicationStatusTrustAnchorEntry::Invalid {
        error: "NoCertificateFound".into(),
        message: "No PEM encoded certificate was found".into(),
    })
}
