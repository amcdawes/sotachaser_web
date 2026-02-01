mod app;
mod serial;

use wasm_bindgen::prelude::*;
use yew::Renderer;

#[wasm_bindgen(start)]
pub fn run() {
    Renderer::<app::App>::new().render();
}
