// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example `hyper`-based HTTP client which parses a multipart stream.
//! Run with the URL printed by `cargo run --example server` as its one argument.

use tokio::task::JoinHandle;

use mjpeg_proxy::config;

#[tokio::main]
async fn main() {
    let servers = config::parse_config("config.yml").expect("Failed to read config");
    let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(servers.len());

    for (_server_name, mut server) in servers {
        server.init();

        let handle = tokio::spawn(async move { server.run().await });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }
}
