use leptos::prelude::*;

/// xterm.js terminal wrapper.
///
/// Renders a container div and calls `window.initTerminal()` from
/// the inline script in the shell HTML. The JS handles WebSocket
/// connection to the PTY bridge at `/api/terminal/ws`.
///
/// ## Props
/// - `folder`: Absolute path to the project directory where the PTY should spawn
/// - `container_id`: Unique ID for this terminal instance (e.g., "terminal-1")
#[component]
pub fn Terminal(
    #[prop(into)] folder: String,
    #[prop(into)] container_id: String,
) -> impl IntoView {
    // folder is used only in the hydrate feature (client-side JS init)
    let _folder = folder;

    // Initialize xterm.js after the element is mounted
    #[cfg(feature = "hydrate")]
    {
        let container_id_clone = container_id.clone();
        let folder_clone = _folder;
        Effect::new(move || {
            use wasm_bindgen::prelude::*;

            let window = web_sys::window().unwrap();
            let container_id_inner = container_id_clone.clone();
            let folder_inner = folder_clone.clone();

            let cb = Closure::once(move || {
                let window = web_sys::window().unwrap();
                if let Ok(func) = js_sys::Reflect::get(&window, &JsValue::from_str("initTerminal"))
                {
                    if func.is_function() {
                        let func: js_sys::Function = func.into();
                        let _ = func.call2(
                            &JsValue::NULL,
                            &JsValue::from_str(&container_id_inner),
                            &JsValue::from_str(&folder_inner),
                        );
                    }
                }
            });
            // Use setTimeout instead of rAF to ensure Leptos has flushed DOM updates after hydration
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                50,
            );
            cb.forget();

            // Cleanup: dispose terminal when component unmounts
            let container_id_cleanup = container_id_clone.clone();
            on_cleanup(move || {
                let window = web_sys::window().unwrap();
                if let Ok(func) = js_sys::Reflect::get(&window, &JsValue::from_str("disposeTerminal"))
                {
                    if func.is_function() {
                        let func: js_sys::Function = func.into();
                        let _ = func.call1(&JsValue::NULL, &JsValue::from_str(&container_id_cleanup));
                    }
                }
            });
        });
    }

    view! {
        <div class="terminal-wrapper">
            <div id=container_id class="terminal-container"></div>
        </div>
    }
}
