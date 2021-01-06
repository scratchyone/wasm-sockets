//! This crate offers 2 (wasm-only) websocket clients.
//! The first client offered is the [`EventClient`]. This client is event based and gives you the most control.
//! ```
//! use console_error_panic_hook;
//! use console_log;
//! use log::{error, info, Level};
//! use std::panic;
//! use wasm_sockets::{self, WebSocketError};
//!
//! fn main() -> Result<(), WebSocketError> {
//!     panic::set_hook(Box::new(console_error_panic_hook::hook));
//!     // console_log and log macros are used instead of println!
//!     // so that messages can be seen in the browser console
//!     console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
//!     info!("Creating connection");
//!
//!     let mut client = wasm_sockets::EventClient::new("wss://echo.websocket.org")?;
//!     client.set_on_error(Some(Box::new(|error| {
//!         error!("{:#?}", error);
//!     })));
//!     client.set_on_connection(Some(Box::new(|client: &wasm_sockets::EventClient| {
//!         info!("{:#?}", client.status);
//!         info!("Sending message...");
//!         client.send_string("Hello, World!").unwrap();
//!         client.send_binary(vec![20]).unwrap();
//!     })));
//!     client.set_on_close(Some(Box::new(|| {
//!         info!("Connection closed");
//!     })));
//!     client.set_on_message(Some(Box::new(
//!         |client: &wasm_sockets::EventClient, message: wasm_sockets::Message| {
//!             info!("New Message: {:#?}", message);
//!         },
//!     )));
//!
//!     info!("Connection successfully created");
//!     Ok(())
//! }
//! ```
//! The second client offered is the [`PollingClient`]. This client is ideal for games, because it is designed to be used with a loop.
//! This client is also much simpler than the [`EventClient`]. However, you can access the main [`EventClient`] that it is using
//! if you want access to lower level control.
//! ```
//! use console_error_panic_hook;
//! use log::{info, Level};
//! use std::cell::RefCell;
//! use std::panic;
//! use std::rc::Rc;
//! #[cfg(target_arch = "wasm32")]
//! use wasm_bindgen::prelude::*;
//! use wasm_sockets::{self, ConnectionStatus, WebSocketError};
//!
//! fn main() -> Result<(), WebSocketError> {
//!     panic::set_hook(Box::new(console_error_panic_hook::hook));
//!     // console_log and log macros are used instead of println!
//!     // so that messages can be seen in the browser console
//!     console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
//!     info!("Creating connection");
//!
//!     // Client is wrapped in an Rc<RefCell<>> so it can be used within setInterval
//!     // This isn't required when being used within a game engine
//!     let client = Rc::new(RefCell::new(wasm_sockets::PollingClient::new(
//!         "wss://echo.websocket.org",
//!     )?));
//!
//!     let f = Closure::wrap(Box::new(move || {
//!         if client.borrow().status() == ConnectionStatus::Connected {
//!             info!("Sending message");
//!             client.borrow().send_string("Hello, World!").unwrap();
//!         }
//!         // receive() gives you all new websocket messages since receive() was last called
//!         info!("New messages: {:#?}", client.borrow_mut().receive());
//!     }) as Box<dyn FnMut()>);
//!
//!     // Start non-blocking game loop
//!     setInterval(&f, 100);
//!     f.forget();
//!
//!     Ok(())
//! }
//! // Bind setInterval to make a basic game loop
//! #[wasm_bindgen]
//! extern "C" {
//!     fn setInterval(closure: &Closure<dyn FnMut()>, time: u32) -> i32;
//! }
//! ```
#[cfg(test)]
mod tests;
use log::{error, trace};
use std::cell::RefCell;
use std::rc::Rc;
use thiserror::Error;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    /// Connecting to a server
    Connecting,
    /// Connected to a server
    Connected,
    /// Disconnected from a server due to an error
    Error,
    /// Disconnected from a server without an error
    Disconnected,
}

/// Message is a representation of a websocket message that can be sent or recieved
#[derive(Debug, Clone)]
pub enum Message {
    /// A text message
    Text(String),
    /// A binary message
    Binary(Vec<u8>),
}
pub struct PollingClient {
    /// The URL this client is connected to
    pub url: String,
    /// The core [`EventClient`] this client is using
    pub event_client: EventClient,
    /// The current connection status
    pub status: Rc<RefCell<ConnectionStatus>>,
    data: Rc<RefCell<Vec<Message>>>,
}
// TODO: Replace unwraps and JsValue with custom error type
impl PollingClient {
    /// Create a new PollingClient and connect to a WebSocket URL
    ///
    /// Note: An Ok() from this function does not mean the connection has succeeded.
    /// ```
    /// PollingClient::new("wss://echo.websocket.org")?;
    /// ```
    pub fn new(url: &str) -> Result<Self, WebSocketError> {
        // Create connection
        let mut client = EventClient::new(url)?;
        let data = Rc::new(RefCell::new(vec![]));
        let data_ref = data.clone();
        let status = Rc::new(RefCell::new(ConnectionStatus::Connecting));
        let status_ref = status.clone();

        client.set_on_connection(Some(Box::new(move |_client| {
            *status_ref.borrow_mut() = ConnectionStatus::Connected;
        })));

        let status_ref = status.clone();

        client.set_on_error(Some(Box::new(move |e| {
            *status_ref.borrow_mut() = ConnectionStatus::Error;
        })));

        let status_ref = status.clone();

        client.set_on_close(Some(Box::new(move || {
            *status_ref.borrow_mut() = ConnectionStatus::Disconnected;
        })));

        client.set_on_message(Some(Box::new(move |_client: &EventClient, m: Message| {
            data_ref.borrow_mut().push(m);
        })));

        Ok(Self {
            url: url.to_string(),
            event_client: client,
            status,
            data,
        })
    }
    /// Get all new WebSocket messages that were received since this function was last called
    /// ```
    /// println!("New messages: {:#?}", client.receive());
    /// ```
    pub fn receive(&mut self) -> Vec<Message> {
        let data = (*self.data.borrow()).clone();
        (*self.data.borrow_mut()).clear();
        data
    }
    /// Get the client's current connection status
    /// ```
    /// println!("Current status: {:#?}", client.status());
    /// ```
    pub fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }
    /// Send a text message to the server
    /// ```
    /// client.send_string("Hello server!")?;
    /// ```
    pub fn send_string(&self, message: &str) -> Result<(), JsValue> {
        self.event_client.send_string(message)
    }
    /// Send a binary message to the server
    /// ```
    /// client.send_binary(vec![0x2, 0xF])?;
    /// ```
    pub fn send_binary(&self, message: Vec<u8>) -> Result<(), JsValue> {
        self.event_client.send_binary(message)
    }
}

#[derive(Debug, Clone, Error)]
pub enum WebSocketError {
    #[error("Failed to create websocket connection: {0}")]
    ConnectionCreationError(String),
}

#[cfg(target_arch = "wasm32")]
pub struct EventClient {
    /// The URL this client is connected to
    pub url: Rc<RefCell<String>>,
    /// The raw web_sys WebSocket object this client is using.
    /// Be careful when using this field, as it will be a different type depending on the compilation target.
    connection: Rc<RefCell<web_sys::WebSocket>>,
    /// The current connection status
    pub status: Rc<RefCell<ConnectionStatus>>,
    /// The function bound to the on_error event
    pub on_error: Rc<RefCell<Option<Box<dyn Fn(ErrorEvent) -> ()>>>>,
    /// The function bound to the on_connection event
    pub on_connection: Rc<RefCell<Option<Box<dyn Fn(&EventClient) -> ()>>>>,
    /// The function bound to the on_message event
    pub on_message: Rc<RefCell<Option<Box<dyn Fn(&EventClient, Message) -> ()>>>>,
    /// The function bound to the on_close event
    pub on_close: Rc<RefCell<Option<Box<dyn Fn() -> ()>>>>,
}

#[cfg(target_arch = "wasm32")]
impl EventClient {
    /// Create a new EventClient and connect to a WebSocket URL
    ///
    /// Note: An Ok() from this function does not mean the connection has succeeded.
    /// ```
    /// EventClient::new("wss://echo.websocket.org")?;
    /// ```
    pub fn new(url: &str) -> Result<Self, WebSocketError> {
        // Create connection
        let ws: web_sys::WebSocket = match WebSocket::new(url) {
            Ok(ws) => ws,
            Err(e) => Err(WebSocketError::ConnectionCreationError(
                "Failed to connect".into(),
            ))?,
        };
        // For small binary messages, like CBOR, Arraybuffer is more efficient than Blob handling
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let status = Rc::new(RefCell::new(ConnectionStatus::Connecting));
        let ref_status = status.clone();

        let on_error: Rc<RefCell<Option<Box<dyn Fn(ErrorEvent) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_error_ref = on_error.clone();

        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            *ref_status.borrow_mut() = ConnectionStatus::Error;
            if let Some(f) = &*on_error_ref.borrow() {
                f.as_ref()(e);
            }
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let on_close: Rc<RefCell<Option<Box<dyn Fn() -> ()>>>> = Rc::new(RefCell::new(None));
        let on_close_ref = on_close.clone();
        let ref_status = status.clone();

        let onclose_callback = Closure::wrap(Box::new(move || {
            *ref_status.borrow_mut() = ConnectionStatus::Disconnected;
            if let Some(f) = &*on_close_ref.borrow() {
                f.as_ref()();
            }
        }) as Box<dyn FnMut()>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        let on_connection: Rc<RefCell<Option<Box<dyn Fn(&EventClient) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_connection_ref = on_connection.clone();

        let on_message: Rc<RefCell<Option<Box<dyn Fn(&EventClient, Message) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_message_ref = on_message.clone();

        let ref_status = status.clone();

        let connection = Rc::new(RefCell::new(ws));

        let client = Rc::new(RefCell::new(Self {
            url: Rc::new(RefCell::new(url.to_string())),
            connection: connection.clone(),
            on_error: on_error.clone(),
            on_connection: on_connection.clone(),
            status: status.clone(),
            on_message: on_message.clone(),
            on_close: on_close.clone(),
        }));
        let client_ref = client.clone();

        let onopen_callback = Closure::wrap(Box::new(move |_| {
            *ref_status.borrow_mut() = ConnectionStatus::Connected;
            if let Some(f) = &*on_connection_ref.borrow() {
                f.as_ref()(&*client_ref.clone().borrow());
            }
        }) as Box<dyn FnMut(JsValue)>);
        connection
            .borrow_mut()
            .set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let client_ref = client.clone();

        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            // Process different types of message data
            if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                // Received arraybuffer
                trace!("message event, received arraybuffer: {:?}", abuf);
                // Convert arraybuffer to vec
                let array = js_sys::Uint8Array::new(&abuf).to_vec();
                if let Some(f) = &*on_message_ref.borrow() {
                    f.as_ref()(&*client_ref.clone().borrow(), Message::Binary(array));
                }
            } else if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
                // Received blob data
                trace!("message event, received blob: {:?}", blob);
                let fr = web_sys::FileReader::new().unwrap();
                let fr_c = fr.clone();
                // create onLoadEnd callback
                let cbref = on_message_ref.clone();
                let cbfref = client_ref.clone();
                let onloadend_cb = Closure::wrap(Box::new(move |_e: web_sys::ProgressEvent| {
                    let array = js_sys::Uint8Array::new(&fr_c.result().unwrap()).to_vec();
                    if let Some(f) = &*cbref.borrow() {
                        f.as_ref()(&*cbfref.clone().borrow(), Message::Binary(array));
                    }
                })
                    as Box<dyn FnMut(web_sys::ProgressEvent)>);
                fr.set_onloadend(Some(onloadend_cb.as_ref().unchecked_ref()));
                fr.read_as_array_buffer(&blob).expect("blob not readable");
                onloadend_cb.forget();
            } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                if let Some(f) = &*on_message_ref.borrow() {
                    f.as_ref()(&*client_ref.clone().borrow(), Message::Text(txt.into()));
                }
            } else {
                // Got unknown data
                panic!("Unknown data: {:#?}", e.data());
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        // set message event handler on WebSocket
        connection
            .borrow()
            .set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        // forget the callback to keep it alive
        onmessage_callback.forget();

        Ok(Self {
            url: Rc::new(RefCell::new(url.to_string())),
            connection,
            on_error,
            on_connection,
            on_message,
            on_close,
            status: status,
        })
    }
    /// Set an on_error event handler.
    /// This handler will be run when the client disconnects from the server due to an error.
    /// This will overwrite the previous handler.
    /// You can set [None](std::option) to disable the on_error handler.
    /// ```
    /// client.set_on_error(Some(Box::new(|error| {
    ///    panic!("Error: {:#?}", error);
    /// })));
    /// ```
    pub fn set_on_error(&mut self, f: Option<Box<dyn Fn(ErrorEvent) -> ()>>) {
        *self.on_error.borrow_mut() = f;
    }
    /// Set an on_connection event handler.
    /// This handler will be run when the client successfully connects to a server.
    /// This will overwrite the previous handler.
    /// You can set [None](std::option) to disable the on_connection handler.
    /// ```
    /// client.set_on_connection(Some(Box::new(|client| {
    ///     info!("Connected");
    /// })));
    /// ```
    pub fn set_on_connection(&mut self, f: Option<Box<dyn Fn(&EventClient) -> ()>>) {
        *self.on_connection.borrow_mut() = f;
    }
    /// Set an on_message event handler.
    /// This handler will be run when the client receives a message from a server.
    /// This will overwrite the previous handler.
    /// You can set [None](std::option) to disable the on_message handler.
    /// ```
    /// client.set_on_message(Some(Box::new(
    ///     |c, m| {
    ///         info!("New Message: {:#?}", m);
    ///     },
    ///  )));
    /// ```
    pub fn set_on_message(&mut self, f: Option<Box<dyn Fn(&EventClient, Message) -> ()>>) {
        *self.on_message.borrow_mut() = f;
    }
    /// Set an on_close event handler.
    /// This handler will be run when the client disconnects from a server without an error.
    /// This will overwrite the previous handler.
    /// You can set [None](std::option) to disable the on_close handler.
    /// ```
    /// client.set_on_close(Some(Box::new(|| {
    ///     info!("Closed");
    /// })));
    /// ```
    pub fn set_on_close(&mut self, f: Option<Box<dyn Fn() -> ()>>) {
        *self.on_close.borrow_mut() = f;
    }

    /// Send a text message to the server
    /// ```
    /// client.send_string("Hello server!")?;
    /// ```
    pub fn send_string(&self, message: &str) -> Result<(), JsValue> {
        self.connection.borrow().send_with_str(message)
    }
    /// Send a binary message to the server
    /// ```
    /// client.send_binary(vec![0x2, 0xF])?;
    /// ```
    pub fn send_binary(&self, message: Vec<u8>) -> Result<(), JsValue> {
        self.connection
            .borrow()
            .send_with_u8_array(message.as_slice())
    }
}
