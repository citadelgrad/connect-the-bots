use leptos::prelude::*;

/// xterm.js terminal wrapper.
///
/// Renders a container div and calls `window.initTerminal()` from
/// the inline script in the shell HTML. The JS handles WebSocket
/// connection to the PTY bridge at `/api/terminal/ws`.
#[component]
pub fn Terminal() -> impl IntoView {
    let container_id = "terminal-container";

    // Initialize xterm.js after the element is mounted
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move || {
            use wasm_bindgen::prelude::*;

            let window = web_sys::window().unwrap();
            let cb = Closure::once(move || {
                let window = web_sys::window().unwrap();
                if let Ok(func) = js_sys::Reflect::get(&window, &JsValue::from_str("initTerminal"))
                {
                    if func.is_function() {
                        let func: js_sys::Function = func.into();
                        let _ = func.call1(&JsValue::NULL, &JsValue::from_str(container_id));
                    }
                }
            });
            let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
            cb.forget();
        });
    }

    view! {
        <div class="terminal-wrapper">
            <div id=container_id class="terminal-container"></div>
        </div>
    }
}
