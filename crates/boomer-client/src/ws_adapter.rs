//! Browser WebSocket [`GameTransport`] adapter for Step 2 online play (`web-sys`).

#![allow(
    unsafe_code,
    reason = "wasm-bindgen Closure interop with browser WebSocket events"
)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use boomer_protocol::{ClientMessage, ServerMessage};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket};

use crate::ports::{GameTransport, TransportError};

const INITIAL_BACKOFF_MS: u32 = 1_000;
const MAX_BACKOFF_MS: u32 = 15_000;

struct SharedWsState {
    inbox: VecDeque<ServerMessage>,
    pending_outbox: VecDeque<String>,
    open: bool,
    closed: bool,
}

#[expect(
    dead_code,
    reason = "holds JS Closure handles alive for the lifetime of the WebSocket"
)]
struct WsClosures {
    open: Closure<dyn FnMut(Event)>,
    message: Closure<dyn FnMut(MessageEvent)>,
    close: Closure<dyn FnMut(CloseEvent)>,
}

/// Step 2 online [`GameTransport`] adapter talking JSON text frames to `boomer-server /ws`.
pub struct WebSocketAdapter {
    url: String,
    socket: Option<WebSocket>,
    shared: Rc<RefCell<SharedWsState>>,
    reconnect_wait_ms: u32,
    backoff_ms: u32,
    closures: Option<WsClosures>,
}

impl Drop for WebSocketAdapter {
    fn drop(&mut self) {
        if let Some(ws) = self.socket.take() {
            ws.set_onopen(None);
            ws.set_onmessage(None);
            ws.set_onclose(None);
            let _ = ws.close();
        }
    }
}

impl WebSocketAdapter {
    /// Creates a new adapter targeting `ws_url` (e.g., `"wss://example.com/ws"`).
    #[must_use]
    pub fn new(ws_url: String) -> Self {
        let mut adapter = Self {
            url: ws_url,
            socket: None,
            shared: Rc::new(RefCell::new(SharedWsState {
                inbox: VecDeque::new(),
                pending_outbox: VecDeque::new(),
                open: false,
                closed: false,
            })),
            reconnect_wait_ms: 0,
            backoff_ms: INITIAL_BACKOFF_MS,
            closures: None,
        };
        adapter.connect();
        adapter
    }

    fn connect(&mut self) {
        if let Some(prev) = self.socket.take() {
            prev.set_onopen(None);
            prev.set_onmessage(None);
            prev.set_onclose(None);
            let _ = prev.close();
        }
        let Ok(ws) = WebSocket::new(&self.url) else {
            self.shared.borrow_mut().closed = true;
            return;
        };

        let shared_open = Rc::clone(&self.shared);
        let ws_open = ws.clone();
        let on_open = Closure::<dyn FnMut(Event)>::new(move |_ev: Event| {
            let mut st = shared_open.borrow_mut();
            st.open = true;
            st.closed = false;
            while let Some(raw) = st.pending_outbox.pop_front() {
                if ws_open.send_with_str(&raw).is_err() {
                    st.pending_outbox.push_front(raw);
                    break;
                }
            }
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let shared_msg = Rc::clone(&self.shared);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
            if let Some(text) = ev.data().as_string()
                && let Ok(parsed) = serde_json::from_str::<ServerMessage>(&text)
            {
                shared_msg.borrow_mut().inbox.push_back(parsed);
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let shared_close = Rc::clone(&self.shared);
        let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |_ev: CloseEvent| {
            let mut st = shared_close.borrow_mut();
            st.open = false;
            st.closed = true;
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        self.socket = Some(ws);
        self.closures = Some(WsClosures {
            open: on_open,
            message: on_message,
            close: on_close,
        });
    }
}

impl GameTransport for WebSocketAdapter {
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(&msg)
            .map_err(|err| TransportError::InvalidMessage(err.to_string()))?;

        let is_open = self.shared.borrow().open;
        if is_open
            && let Some(ws) = &self.socket
            && ws.send_with_str(&json).is_ok()
        {
            return Ok(());
        }

        let mut st = self.shared.borrow_mut();
        if st.pending_outbox.len() < 64 {
            st.pending_outbox.push_back(json);
            Ok(())
        } else {
            Err(TransportError::Disconnected)
        }
    }

    fn poll_messages(&mut self) -> Vec<ServerMessage> {
        self.shared.borrow_mut().inbox.drain(..).collect()
    }

    fn tick(&mut self, dt_ms: u32) {
        let should_reconnect = {
            let st = self.shared.borrow();
            st.closed && !st.open
        };
        if !should_reconnect {
            self.backoff_ms = INITIAL_BACKOFF_MS;
            self.reconnect_wait_ms = 0;
            return;
        }
        self.reconnect_wait_ms = self.reconnect_wait_ms.saturating_add(dt_ms);
        if self.reconnect_wait_ms >= self.backoff_ms {
            self.reconnect_wait_ms = 0;
            self.backoff_ms = self.backoff_ms.saturating_mul(2).min(MAX_BACKOFF_MS);
            self.shared.borrow_mut().closed = false;
            self.connect();
        }
    }
}
