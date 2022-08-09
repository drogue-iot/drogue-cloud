pub mod admin;
pub mod auth;
pub mod endpoints;
mod id;
pub mod kafka;
pub mod labels;
pub mod serde;
pub mod services;
pub mod token;
pub mod version;

pub use id::*;

#[cfg(feature = "actix")]
pub mod webapp;

pub use drogue_bazaar::health;
pub use version::PROJECT;
