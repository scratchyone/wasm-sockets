use console_error_panic_hook;
use log::{info, Level};
use std::cell::RefCell;
use std::panic;
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use wasm_sockets::{self, ConnectionStatus, WebSocketError};

fn main() -> Result<(), WebSocketError> {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    // console_log and log macros are used instead of println!
    // so that messages can be seen in the browser console
    console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
    info!("Creating connection");

    // Client is wrapped in an Rc<RefCell<>> so it can be used within setInterval
    // This isn't required when being used within a game engine
    let client = Rc::new(RefCell::new(wasm_sockets::PollingClient::new(
        "wss://ws.ifelse.io",
    )?));

    let f = Closure::wrap(Box::new(move || {
        if client.borrow().status() == ConnectionStatus::Connected {
            info!("Sending message");
            client.borrow().send_string("Hello, World!").unwrap();
        }
        // receive() gives you all new websocket messages since receive() was last called
        info!("New messages: {:#?}", client.borrow_mut().receive());
    }) as Box<dyn FnMut()>);

    // Start non-blocking game loop
    setInterval(&f, 100);
    f.forget();

    Ok(())
}
// Bind setInterval to make a basic game loop
#[wasm_bindgen]
extern "C" {
    fn setInterval(closure: &Closure<dyn FnMut()>, time: u32) -> i32;
}
