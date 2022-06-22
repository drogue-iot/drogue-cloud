/// Something that can turn into [`PassFail`]
pub trait AsPassFail {
    fn as_pass_fail(&self) -> PassFail;
}

/// An indication if something passed or failed.
pub enum PassFail {
    Pass,
    Fail,
}

/// Something that can turn into [`PassFailError`]
pub trait AsPassFailError {
    fn as_pass_fail_error(&self) -> PassFailError;
}

/// An indication if something passed, failed, or could not be evaluated due to an error.
pub enum PassFailError {
    Pass,
    Fail,
    Error,
}

impl<T, E> AsPassFailError for Result<T, E>
where
    T: AsPassFail,
{
    fn as_pass_fail_error(&self) -> PassFailError {
        match self {
            Ok(outcome) => match outcome.as_pass_fail() {
                PassFail::Pass => PassFailError::Pass,
                PassFail::Fail => PassFailError::Fail,
            },
            Err(_) => PassFailError::Error,
        }
    }
}
