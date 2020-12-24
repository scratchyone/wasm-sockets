use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);
use crate as wasm_sockets;
#[cfg(test)]
use console_error_panic_hook;
#[cfg(test)]
use console_log;
use log::{error, info, Level};
use std::panic;
use wasm_bindgen::JsValue;

#[wasm_bindgen_test]
fn event() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    // console_log and log macros are used instead of println!
    // so that messages can be seen in the browser console
    console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
    info!("Creating connection");

    let mut client = wasm_sockets::EventClient::new("wss://echo.websocket.org").unwrap();
    client.set_on_error(Some(Box::new(|error| {
        error!("{:#?}", error);
    })));
    client.set_on_connection(Some(Box::new(|client: &wasm_sockets::EventClient| {
        info!("{:#?}", client.status);
        info!("Sending message...");
        client.send_string("Hello, World!").unwrap();
        client.send_binary(vec![20]).unwrap();
    })));
    client.set_on_close(Some(Box::new(|| {
        info!("Connection closed");
    })));
    client.set_on_message(Some(Box::new(
        |client: &wasm_sockets::EventClient, message: wasm_sockets::Message| {
            info!("New Message: {:#?}", message);
        },
    )));
    info!("Connection successfully created");
}
