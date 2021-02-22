mod app;
mod device;
mod meta;

pub use app::*;
pub use device::*;
pub use meta::*;

use base64_serde::base64_serde_type;

base64_serde_type!(Base64Standard, base64::STANDARD);

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
