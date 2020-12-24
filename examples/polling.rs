use console_error_panic_hook;
use log::{debug, error, info, trace, warn, Level};
use std::cell::RefCell;
use std::panic;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures;
use wasm_sockets::{self, ConnectionStatus};

fn main() -> Result<(), JsValue> {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
    info!("Creating connection");

    // Client is wrapped in an Rc<RefCell<>> so it can be used within setInterval
    let client = Rc::new(RefCell::new(wasm_sockets::PollingClient::new(
        "wss://echo.websocket.org",
    )?));

    let f = Closure::wrap(Box::new(move || {
        info!("{:#?}", client.borrow_mut().receive());
        info!("{:#?}", client.borrow().status());
    }) as Box<dyn FnMut()>);
    setInterval(&f, 100); // Create non-blocking loop
    f.forget();

    Ok(())
}
#[wasm_bindgen]
extern "C" {
    fn setInterval(closure: &Closure<dyn FnMut()>, time: u32) -> i32;
}
