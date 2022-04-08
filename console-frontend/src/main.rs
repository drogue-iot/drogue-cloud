#![recursion_limit = "1024"]
#![allow(clippy::needless_return)]
mod app;
mod backend;
mod components;
mod data;
mod error;
mod examples;
mod page;
mod pages;
mod preferences;
mod utils;

use crate::app::Application;
use wasm_bindgen::prelude::*;

#[cfg(not(feature = "debug"))]
const LOG_LEVEL: log::Level = log::Level::Info;
#[cfg(feature = "debug")]
const LOG_LEVEL: log::Level = log::Level::Trace;

pub fn main() -> Result<(), JsValue> {
    wasm_logger::init(wasm_logger::Config::new(LOG_LEVEL));
    log::info!("Getting ready...");
    yew::start_app::<Application>();
    Ok(())
}
