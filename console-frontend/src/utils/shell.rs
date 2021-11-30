pub fn shell_quote<S: ToString>(s: S) -> String {
    s.to_string().replace('\\', "\\\\").replace('\'', "\\\'")
}

/// Escape into single-quote string
pub fn shell_single_quote<S: ToString>(s: S) -> String {
    let s = s.to_string();

    if s.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
    {
        s
    } else {
        format!("'{}'", s.replace('\'', r#"'"'"'"#))
    }
}

#[cfg(test)]
mod test {
    use crate::utils::shell::shell_single_quote;

    #[test]
    fn test_single() {
        for (i, o) in [
            ("", ""),
            ("foo", "foo"),
            ("f-_o", "f-_o"),
            ("ba r", "'ba r'"),
        ] {
            assert_eq!(o, shell_single_quote(i))
        }
    }
}
