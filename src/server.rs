use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::header::CONTENT_TYPE,
    response::Response,
    routing::get,
    Error, Router,
};
use chrono::Local;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use crate::connection::Connection;

const BOUNDARY_STRING: &str = "-----mjpeg-proxy-boundary-----";

const fn _default_fps() -> u8 {
    25
}

const fn _default_port() -> u16 {
    8080
}

fn _localhost() -> String {
    "127.0.0.1".to_string()
}

async fn serve(
    State(config): State<Arc<HashMap<String, Arc<Connection>>>>,
    ConnectInfo(raddr): ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    let path = req.uri().to_string();
    let version = req.version();
    let date = Local::now();
    let method = req.method().to_string();
    let res = config.get(&path);
    if let Some(r) = res {
        println!(
            "{} - - [{}] \"{} {} {:?}\" 200 -",
            raddr.ip(),
            date.format("%d/%m/%Y:%H:%M:%S %z"),
            method,
            path,
            version
        );
        let recv = Connection::subscribe(Arc::clone(r));
        let stream = multipart_stream::serialize(
            recv.map(|p| -> Result<_, Box<Error>> { Ok(p) }),
            BOUNDARY_STRING,
        );
        Response::builder()
            .header(
                CONTENT_TYPE,
                "multipart/x-mixed-replace; boundary=".to_string() + BOUNDARY_STRING,
            )
            .body(Body::from_stream(stream))
            .unwrap() // TODO: Proper error handling
    } else {
        println!(
            "{} - - [{}] \"{} {} {:?}\" 404 13",
            raddr.ip(),
            date.format("%d/%m/%Y:%H:%M:%S %z"),
            method,
            path,
            version
        );
        println!("404 returned");
        Response::builder()
            .status(404)
            .body(Body::from("404 Not found"))
            .unwrap() // TODO: Proper error handling
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamConfig {
    #[serde(default = "_default_fps")]
    fps: u8,
    url: String,
    path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Server {
    #[serde(default = "_default_port")]
    port: u16,
    #[serde(default = "_localhost")]
    address: String,
    #[serde(rename(serialize = "streams", deserialize = "streams"))]
    stream_configs: HashMap<String, StreamConfig>,
}

impl Server {
    pub fn init(&mut self) {
        for (_stream_name, stream) in &mut self.stream_configs {
            if !stream.path.starts_with("/") {
                stream.path = format!("/{}", stream.path);
            }
        }
    }

    pub async fn run(&self) {
        let addr: SocketAddr = format!("{}:{}", &self.address, self.port).parse().unwrap();
        let mut stream_config = HashMap::new();
        for v in self.stream_configs.iter() {
            stream_config.insert(
                v.1.path.clone(),
                Arc::new(Connection::new(&*v.1.url, v.1.fps)),
            );
            println!("Serving {} on {} fps: {}", v.1.url, v.1.path, v.1.fps);
        }

        let app = Router::new()
            .route("/{*_}", get(serve))
            .with_state(Arc::new(stream_config))
            .into_make_service_with_connect_info::<SocketAddr>();

        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

        axum::serve(listener, app).await.unwrap();
    }
}
