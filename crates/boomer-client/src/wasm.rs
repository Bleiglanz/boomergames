//! Thin `wasm-bindgen` FFI shim exposing [`BoomerClient`] (offline) and
//! [`BoomerRemoteClient`] (Step 2 online multiplayer) to JavaScript.

#![allow(
    unsafe_code,
    reason = "wasm-bindgen proc-macro emits FFI extern blocks internally"
)]

use boomer_protocol::{GameKindDto, ServerMessage};
use js_sys::Float64Array;
use wasm_bindgen::prelude::*;

use crate::app::{ClientApp, RemoteRole, UiInput};
use crate::bridge_adapter::BridgeTransport;
use crate::local_adapter::LocalSessionAdapter;

/// JavaScript-facing offline WASM client handle (Step 1).
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

/// JavaScript-facing authoritative online multiplayer WASM client handle (Step 2).
#[wasm_bindgen]
pub struct BoomerRemoteClient {
    app: ClientApp<BridgeTransport>,
}

#[wasm_bindgen]
impl BoomerRemoteClient {
    /// Creates a new remote game client configured for `game` (`"checkers" | "pool"`)
    /// and `role` (`"seat1" | "seat2" | "spectator"`).
    ///
    /// # Errors
    /// Returns a JS error if `game` or `role` is unrecognized.
    #[wasm_bindgen(constructor)]
    pub fn new(game: &str, role: &str) -> Result<Self, JsValue> {
        let kind = match game {
            "checkers" => GameKindDto::Checkers,
            "pool" => GameKindDto::Pool,
            "chess" => GameKindDto::Chess,
            _ => return Err(JsValue::from_str("unknown game kind")),
        };
        let remote_role = match role {
            "seat1" => RemoteRole::Seat1,
            "seat2" => RemoteRole::Seat2,
            "spectator" => RemoteRole::Spectator,
            _ => return Err(JsValue::from_str("unknown remote role")),
        };
        let mut app = ClientApp::new(BridgeTransport::new());
        app.enter_remote_game(kind, remote_role);
        Ok(Self { app })
    }

    /// Feeds a raw JSON [`ServerMessage`] received from `/ws` into the WASM engine.
    ///
    /// # Errors
    /// Returns a JS error if `msg_json` fails deserialization.
    #[wasm_bindgen(js_name = pushServerMessage)]
    pub fn push_server_message(&mut self, msg_json: &str) -> Result<bool, JsValue> {
        let msg: ServerMessage = serde_json::from_str(msg_json)
            .map_err(|err| JsValue::from_str(&format!("invalid ServerMessage JSON: {err}")))?;
        self.app.transport_mut().push_inbound(msg);
        Ok(self.app.poll_transport())
    }

    /// Drains outgoing [`boomer_protocol::ClientMessage`]s as a JSON array of objects to send on `/ws`.
    ///
    /// # Errors
    /// Returns a JS error if serialization unexpectedly fails.
    #[wasm_bindgen(js_name = drainClientMessagesJson)]
    pub fn drain_client_messages_json(&mut self) -> Result<String, JsValue> {
        let msgs = self.app.transport_mut().drain_outbound();
        serde_json::to_string(&msgs)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize outbound msgs: {err}")))
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

    /// Advances Pool shot replay animation by `dt_ms`. Returns `true` if `ViewModel` changed.
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
