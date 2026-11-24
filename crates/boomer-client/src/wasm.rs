//! Thin `wasm-bindgen` FFI shim exposing [`BoomerClient`] to JavaScript.

#![allow(
    unsafe_code,
    reason = "wasm-bindgen proc-macro emits FFI extern blocks internally"
)]

use js_sys::Float64Array;
use wasm_bindgen::prelude::*;

use crate::app::{ClientApp, UiInput};
use crate::local_adapter::LocalSessionAdapter;

/// JavaScript-facing WASM client handle.
#[wasm_bindgen]
pub struct BoomerClient {
    app: ClientApp<LocalSessionAdapter>,
}

#[wasm_bindgen]
impl BoomerClient {
    /// Creates a new offline Boomergames client instance.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            app: ClientApp::new(LocalSessionAdapter::new()),
        }
    }

    /// Dispatches a JSON-encoded [`UiInput`] event. Returns `true` if `ViewModel` changed.
    ///
    /// # Errors
    /// Returns a JS error string if `input_json` is not valid `UiInput` JSON.
    #[wasm_bindgen(js_name = dispatchInput)]
    pub fn dispatch_input(&mut self, input_json: &str) -> Result<bool, JsValue> {
        let input: UiInput = serde_json::from_str(input_json)
            .map_err(|err| JsValue::from_str(&format!("invalid UiInput JSON: {err}")))?;
        Ok(self.app.handle_input(input))
    }

    /// Advances offline timers and time-sliced AI by `dt_ms`. Returns `true` if `ViewModel` changed.
    #[wasm_bindgen]
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        self.app.tick(dt_ms)
    }

    /// Serializes the current `ViewModel` snapshot to JSON.
    ///
    /// # Errors
    /// Returns a JS error string if serialization unexpectedly fails.
    #[wasm_bindgen(js_name = viewModelJson)]
    pub fn view_model_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(self.app.view_model())
            .map_err(|err| JsValue::from_str(&format!("failed to serialize ViewModel: {err}")))
    }

    /// Returns a flat `Float64Array` of length 64 (`16` balls × `[id, x, y, flags]`) for 60 fps Pool rendering.
    #[wasm_bindgen(js_name = poolFrame)]
    #[must_use]
    pub fn pool_frame(&self, t_ms: f64) -> Float64Array {
        Float64Array::from(self.app.pool_frame(t_ms).as_slice())
    }

    /// Drains pending discrete audio cues as a JSON array string (`"[]"` when empty).
    ///
    /// # Errors
    /// Returns a JS error string if serialization unexpectedly fails.
    #[wasm_bindgen(js_name = drainSoundCuesJson)]
    pub fn drain_sound_cues_json(&mut self) -> Result<String, JsValue> {
        let cues = self.app.drain_sound_cues();
        serde_json::to_string(&cues)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize SoundCue list: {err}")))
    }
}

impl Default for BoomerClient {
    fn default() -> Self {
        Self::new()
    }
}
