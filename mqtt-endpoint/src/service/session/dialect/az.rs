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
            (path.trim_end_matches('/'), vec![])
        }
    } else {
        // single topic segment
        (path, vec![])
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_plain() {
        assert_eq!(split_topic("foo/bar"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_plain_slash() {
        assert_eq!(split_topic("foo/bar/"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_plain_slash_q() {
        assert_eq!(split_topic("foo/bar/?"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_properties() {
        assert_eq!(
            split_topic("foo/bar/?baz=123"),
            ("foo/bar", vec![("baz".into(), "123".into())])
        );
    }
}
