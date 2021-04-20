use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

pub(crate) fn random_password() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}
