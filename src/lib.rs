use log::{debug, error, info, trace, warn, Level};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

pub fn start_websocket() -> Result<(), JsValue> {
    // Connect to an echo server
    let ws: web_sys::WebSocket = WebSocket::new("ws://localhost:9001")?;
    // For small binary messages, like CBOR, Arraybuffer is more efficient than Blob handling
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
    // create callback
    let cloned_ws = ws.clone();
    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        // Handle difference Text/Binary,...
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            info!("message event, received arraybuffer: {:?}", abuf);
            let array = js_sys::Uint8Array::new(&abuf);
            let len = array.byte_length() as usize;
            info!("Arraybuffer received {}bytes: {:?}", len, array.to_vec());
            // here you can for example use Serde Deserialize decode the message
            // for demo purposes we switch back to Blob-type and send off another binary message
            cloned_ws.set_binary_type(web_sys::BinaryType::Blob);
            match cloned_ws.send_with_u8_array(&vec![5, 6, 7, 8]) {
                Ok(_) => info!("binary message successfully sent"),
                Err(err) => info!("error sending message: {:?}", err),
            }
        } else if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
            info!("message event, received blob: {:?}", blob);
            // better alternative to juggling with FileReader is to use https://crates.io/crates/gloo-file
            let fr = web_sys::FileReader::new().unwrap();
            let fr_c = fr.clone();
            // create onLoadEnd callback
            let onloadend_cb = Closure::wrap(Box::new(move |_e: web_sys::ProgressEvent| {
                let array = js_sys::Uint8Array::new(&fr_c.result().unwrap());
                let len = array.byte_length() as usize;
                info!("Blob received {}bytes: {:?}", len, array.to_vec());
                // here you can for example use the received image/png data
            })
                as Box<dyn FnMut(web_sys::ProgressEvent)>);
            fr.set_onloadend(Some(onloadend_cb.as_ref().unchecked_ref()));
            fr.read_as_array_buffer(&blob).expect("blob not readable");
            onloadend_cb.forget();
        } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            info!("message event, received Text: {:?}", txt);
        } else {
            info!("message event, received Unknown: {:?}", e.data());
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    // set message event handler on WebSocket
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    // forget the callback to keep it alive
    onmessage_callback.forget();

    let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
        info!("error event: {:?}", e);
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    let cloned_ws = ws.clone();
    let onopen_callback = Closure::wrap(Box::new(move |_| {
        info!("socket opened");
        match cloned_ws.send_with_str("ping") {
            Ok(_) => info!("message successfully sent"),
            Err(err) => info!("error sending message: {:?}", err),
        }
        // send off binary message
        match cloned_ws.send_with_u8_array(&vec![0, 1, 2, 3]) {
            Ok(_) => info!("binary message successfully sent"),
            Err(err) => info!("error sending message: {:?}", err),
        }
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    Ok(())
}
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Error(ErrorEvent),
}
#[derive(Debug, Clone)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
}
pub struct PollingClient {
    pub url: String,
    pub event_client: EventClient,
    pub status: Rc<RefCell<ConnectionStatus>>,
    pub data: Rc<RefCell<Vec<Message>>>,
}
// TODO: Replace unwraps and JsValue with custom error type
impl PollingClient {
    pub fn new(url: &str) -> Result<Self, JsValue> {
        // Create connection
        let mut client = EventClient::new(url)?;
        let data = Rc::new(RefCell::new(vec![]));
        let data_ref = data.clone();
        let status = Rc::new(RefCell::new(ConnectionStatus::Connecting));
        let status_ref = status.clone();

        client.set_on_connection(Some(Box::new(move |c, e| {
            *status_ref.borrow_mut() = ConnectionStatus::Connected;
        })));

        let status_ref = status.clone();

        client.set_on_error(Some(Box::new(move |e| {
            *status_ref.borrow_mut() = ConnectionStatus::Error(e);
        })));

        client.set_on_message(Some(Box::new(move |c: &EventClient, m: Message| {
            data_ref.borrow_mut().push(m);
        })));

        Ok(Self {
            url: url.to_string(),
            event_client: client,
            status,
            data,
        })
    }
    pub fn receive(&mut self) -> Vec<Message> {
        let data = (*self.data.borrow()).clone();
        (*self.data.borrow_mut()).clear();
        data
    }
    pub fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }

    pub fn send_string(&self, message: &str) -> Result<(), JsValue> {
        self.event_client.send_string(message)
    }

    pub fn send_binary(&self, message: Vec<u8>) -> Result<(), JsValue> {
        self.event_client.send_binary(message)
    }
}
pub struct EventClient {
    pub url: Rc<RefCell<String>>,
    pub connection: Rc<RefCell<web_sys::WebSocket>>,
    pub status: Rc<RefCell<ConnectionStatus>>,
    pub on_error: Rc<RefCell<Option<Box<dyn Fn(ErrorEvent) -> ()>>>>,
    pub on_connection: Rc<RefCell<Option<Box<dyn Fn(&EventClient, JsValue) -> ()>>>>,
    pub on_message: Rc<RefCell<Option<Box<dyn Fn(&EventClient, Message) -> ()>>>>,
}
impl EventClient {
    pub fn new(url: &str) -> Result<Self, JsValue> {
        // Create connection
        let ws: web_sys::WebSocket = WebSocket::new(url)?;
        // For small binary messages, like CBOR, Arraybuffer is more efficient than Blob handling
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let status = Rc::new(RefCell::new(ConnectionStatus::Connecting));
        let ref_status = status.clone();

        let on_error: Rc<RefCell<Option<Box<dyn Fn(ErrorEvent) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_error_ref = on_error.clone();

        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            *ref_status.borrow_mut() = ConnectionStatus::Error(e.clone());
            if let Some(f) = &*on_error_ref.borrow() {
                f.as_ref()(e);
            }
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let on_connection: Rc<RefCell<Option<Box<dyn Fn(&EventClient, JsValue) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_connection_ref = on_connection.clone();

        let on_message: Rc<RefCell<Option<Box<dyn Fn(&EventClient, Message) -> ()>>>> =
            Rc::new(RefCell::new(None));
        let on_message_ref = on_message.clone();

        let ref_status = status.clone();

        let connection = Rc::new(RefCell::new(ws));
        let connection_ref = connection.clone();

        let client = Rc::new(RefCell::new(Self {
            url: Rc::new(RefCell::new(url.to_string())),
            connection: connection.clone(),
            on_error: on_error.clone(),
            on_connection: on_connection.clone(),
            status: status.clone(),
            on_message: on_message.clone(),
        }));
        let client_ref = client.clone();

        let onopen_callback = Closure::wrap(Box::new(move |v| {
            *ref_status.borrow_mut() = ConnectionStatus::Connected;
            if let Some(f) = &*on_connection_ref.borrow() {
                f.as_ref()(&*client_ref.clone().borrow(), v);
            }
        }) as Box<dyn FnMut(JsValue)>);
        connection
            .borrow_mut()
            .set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let client_ref = client.clone();
        let client_ref2 = client.clone();

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
            status: status,
        })
    }

    pub fn set_on_error(&mut self, f: Option<Box<dyn Fn(ErrorEvent) -> ()>>) {
        *self.on_error.borrow_mut() = f;
    }
    pub fn set_on_connection(&mut self, f: Option<Box<dyn Fn(&EventClient, JsValue) -> ()>>) {
        *self.on_connection.borrow_mut() = f;
    }

    pub fn set_on_message(&mut self, f: Option<Box<dyn Fn(&EventClient, Message) -> ()>>) {
        *self.on_message.borrow_mut() = f;
    }

    pub fn send_string(&self, message: &str) -> Result<(), JsValue> {
        self.connection.borrow().send_with_str(message)
    }

    pub fn send_binary(&self, message: Vec<u8>) -> Result<(), JsValue> {
        self.connection
            .borrow()
            .send_with_u8_array(message.as_slice())
    }
}
