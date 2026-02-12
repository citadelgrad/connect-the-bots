mod app;
mod components;
mod pages;

// Server functions need to be available in both ssr and hydrate modes
// The #[server] macro generates client stubs for hydrate mode
pub mod server;

pub use app::App;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
