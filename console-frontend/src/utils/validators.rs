use patternfly_yew::{InputState, ValidationContext, Validator};

/// A validator for non-empty values.
pub fn not_empty() -> Validator<String, InputState> {
    Validator::from(|ctx: ValidationContext<String>| match ctx.value.as_str() {
        "" => InputState::Error,
        _ => InputState::Default,
    })
}
