use drogue_cloud_service_api::events::EventTarget;
use lazy_static::lazy_static;
use regex::Regex;

const MAX_TOPIC_LEN: usize = 63;

const REGEXP: &str = r#"^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$"#;
lazy_static! {
    static ref TOPIC_PATTERN: Regex = Regex::new(REGEXP).expect("Regexp must compile");
}

pub fn make_topic_resource_name(target: EventTarget) -> String {
    let name = match &target {
        EventTarget::Events(app) => {
            let name = format!("events-{}", app);
            // try the simple route, if that works ...
            if name.len() < MAX_TOPIC_LEN && TOPIC_PATTERN.is_match(&name) {
                // ... simply return
                return name;
            } else {
                // otherwise we need to clean up the name, and ensure we don't generate duplicates
                // use a different prefix to prevent clashes with the simple names
                let hash = md5::compute(app);
                format!("evt-{:x}-{}", hash, app)
            }
        }
        EventTarget::Commands(_) => return "iot-commands".to_string(),
    };

    let name: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '-' | 'a'..='z' | '0'..='9' => c,
            _ => '-',
        })
        .take(MAX_TOPIC_LEN)
        .collect();

    let name = name.trim_end_matches('-').to_string();

    name
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn topic_names() {
        for i in [
            ("foo", "events-foo"),
            ("00foo", "events-00foo"),
            (
                "0123456789012345678901234567890123456789012345678901234567890123456789",
                "evt-109eb12c10c45d94ddac8eca7b818bed-01234567890123456789012345",
            ),
            ("FOO", "evt-901890a8e9c8cf6d5a1a542b229febff-foo"),
            ("foo-", "evt-03f19ca8da08c40c2d036c8915d383e2-foo"),
        ] {
            assert_eq!(
                i.1,
                make_topic_resource_name(EventTarget::Events(i.0.to_string()))
            )
        }
    }
}
