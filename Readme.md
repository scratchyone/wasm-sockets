# wasm-sockets

`wasm-sockets` is a WASM only rust websocket library primarily designed for creating games.

This crate offers 2 (wasm-only) websocket clients.
The first client offered is the `EventClient`. This client is event based and gives you the most control.

```rust
use console_error_panic_hook;
use console_log;
use log::{error, info, Level};
use std::panic;
use wasm_sockets::{self, WebSocketError};

fn main() -> Result<(), WebSocketError> {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    // console_log and log macros are used instead of println!
    // so that messages can be seen in the browser console
    console_log::init_with_level(Level::Trace).expect("Failed to enable logging");
    info!("Creating connection");

    let mut client = wasm_sockets::EventClient::new("wss://ws.ifelse.io")?;
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
    Ok(())
}
```

The second client offered is the `PollingClient`. This client is ideal for games, because it is designed to be used with a loop.
This client is also much simpler than the `EventClient`. However, you can access the main `EventClient` that it is using
if you want access to lower level control.

```rust
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
```
