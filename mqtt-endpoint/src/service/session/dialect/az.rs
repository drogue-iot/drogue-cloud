use std::borrow::Cow;

/// Split an Azure topic, which might carry a "bag of properties" as the last topic segment
pub fn split_topic(path: &str) -> (&str, Vec<(Cow<str>, Cow<str>)>) {
    if let Some((topic, last)) = path.rsplit_once('/') {
        // at least two segments
        if last.starts_with("?") {
            // last one is a bag of properties
            let query = url::form_urlencoded::parse(&last.as_bytes()[1..]);
            (topic, query.collect())
        } else {
            // last one is a regular one
            (path, vec![])
        }
    } else {
        // single topic segment
        (path, vec![])
    }
}
