use console_error_panic_hook;
use log::{debug, error, info, trace, warn, Level};
use std::cell::RefCell;
use std::panic;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_sockets;

fn main() -> Result<(), JsValue> {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
    info!("Creating connection");

    let mut client = wasm_sockets::EventClient::new("ws://localhost:9001")?;
    client.set_on_error(Some(Box::new(|e| {
        error!("{:#?}", e);
    })));
    client.set_on_connection(Some(Box::new(
        |c: Rc<RefCell<wasm_sockets::EventClient>>, e| {
            info!("Connected: {:#?}", e);
            info!("{:#?}", &c.borrow_mut().status);
            info!("Sending message...");
            c.borrow().send_string("test...").unwrap();
        },
    )));

    info!("Connection successfully created");
    Ok(())
}
