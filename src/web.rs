#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
pub fn webgpu_available() -> bool {
    // `navigator.gpu` is gated in web-sys behind `web_sys_unstable_apis` and its
    // getter returns `Gpu` directly (not `Option<Gpu>`), so we probe the JS
    // object for the property instead. Reflect::has returns `true` when the
    // browser exposes the WebGPU API.
    web_sys::window()
        .and_then(|w| {
            let nav = w.navigator();
            js_sys::Reflect::has(nav.as_ref(), &"gpu".into()).ok()
        })
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
pub fn show_fallback_message() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(body) = document.body() {
                let el = document
                    .create_element("div")
                    .unwrap()
                    .unchecked_into::<web_sys::HtmlElement>();
                el.set_inner_text(
                    "WebGPU is not available in this browser. \
                     Please use a recent version of Chrome, Edge, or Firefox.",
                );
                el.set_attribute(
                    "style",
                    "position:fixed;inset:0;display:flex;align-items:center;\
                     justify-content:center;font-family:sans-serif;font-size:1.5rem;\
                     text-align:center;padding:2rem;background:#111;color:#eee;",
                )
                .ok();
                body.append_child(&el).ok();
            }
        }
    }
}
