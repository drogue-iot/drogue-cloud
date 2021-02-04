#![recursion_limit = "512"]
#![allow(clippy::needless_return)]
mod app;
mod backend;
mod components;
mod error;
mod index;
mod placeholder;
mod spy;

use crate::app::Main;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_app() -> Result<(), JsValue> {
    wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    log::info!("Getting ready...");
    yew::start_app::<Main>();
    Ok(())
}
