use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, Request, State},
    http::header::CONTENT_TYPE,
    response::Response,
    routing::get,
    Router,
};
use chrono::Local;
use futures::StreamExt;
use lazy_static_include::lazy_static_include_bytes;
use multipart_stream::Part;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{broadcast, watch},
    time::{interval, sleep},
};
use tokio_stream::wrappers::BroadcastStream;

lazy_static_include_bytes! {
    DEFAULT_BODY_CONTENT => "res/no_feed_640x480.jpg",
}

const BOUNDARY_STRING: &str = "-----mjpeg-proxy-boundary-----";

const fn _none<T>() -> Option<T> {
    None
}

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
    State(config): State<Arc<HashMap<String, Arc<Mutex<broadcast::Sender<Part>>>>>>,
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
        let recv = r.lock().unwrap().subscribe();
        let stream = BroadcastStream::new(recv);
        let stream = multipart_stream::serialize(stream, BOUNDARY_STRING);
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
pub struct Stream {
    #[serde(default = "_default_fps")]
    pub fps: u8,
    pub url: String,
    pub path: String,
    #[serde(skip, default = "_none::<Arc<Mutex<broadcast::Sender<Part>>>>")]
    pub broadcaster: Option<Arc<Mutex<broadcast::Sender<Part>>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Server {
    #[serde(default = "_default_port")]
    pub port: u16,
    #[serde(default = "_localhost")]
    pub address: String,
    pub streams: HashMap<String, Stream>,
}

impl Server {
    pub fn init(&mut self) {
        for (_stream_name, stream) in &mut self.streams {
            if !stream.path.starts_with("/") {
                stream.path = format!("/{}", stream.path);
            }
            let (tx, _) = broadcast::channel(1);
            stream.broadcaster = Some(Arc::new(Mutex::new(tx)));
        }
    }

    pub async fn run(&self) {
        let addr: SocketAddr = format!("{}:{}", &self.address, self.port).parse().unwrap();
        let mut handles = Vec::new();
        let mut stream_config = Arc::new(HashMap::new());
        let mut config = HashMap::new();
        for v in self.streams.iter() {
            Arc::get_mut(&mut stream_config).unwrap().insert(
                v.1.path.clone(),
                Arc::clone(v.1.broadcaster.as_ref().unwrap()),
            );
            config.insert(v.1.path.clone(), v.1.clone());
        }

        let app = Router::new()
            .route("/{*_}", get(serve))
            .with_state(stream_config)
            .into_make_service_with_connect_info::<SocketAddr>();

        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        for stream in config.into_iter() {
            let default_body = Bytes::from(&DEFAULT_BODY_CONTENT[..]);
            let mut headers = multipart_http::HeaderMap::new();
            headers.insert(
                multipart_http::header::CONTENT_TYPE,
                "image/jpeg".parse().unwrap(),
            );
            headers.insert(
                multipart_http::header::CONTENT_LENGTH,
                "8857".parse().unwrap(),
            );
            let default_part = Part {
                headers,
                body: default_body,
            };
            println!(
                "Serving {} on {} fps: {}",
                stream.1.url, stream.1.path, stream.1.fps
            );

            // start server broadcast channel writer
            let (tx, mut rx) = watch::channel(default_part.clone());
            tokio::spawn(async move {
                // Use the equivalent of a "do-while" loop so the initial value is
                // processed before awaiting the `changed()` future.
                let mut interval = interval(Duration::from_micros(1000000 / stream.1.fps as u64));
                loop {
                    tokio::select!(
                        _ = interval.tick() => {
                            let latest = rx.borrow().clone();
                            //println!("recv: {:?}", latest.headers);
                            let bc = stream.1.broadcaster.as_ref().unwrap();
                            let btx = bc.lock().unwrap();
                            let _res = btx.send(latest);
                            // ignore errors
                            /*if res.is_err() {
                             *                   println!("{:?}", res.err());
                        }*/
                        }
                        res = rx.changed() => {
                            if res.is_err() {
                                break;
                            }
                        }
                    )
                }
            });

            // start reading camera
            let handle = tokio::spawn(async move {
                let mut backoff_secs = 1;
                loop {
                    let res = reqwest::get(&stream.1.url).await;
                    if res.is_err() {
                        eprintln!("err: {:?}", res.err().unwrap());
                        eprintln!("trying again in {} seconds.", backoff_secs);
                        tx.send(default_part.clone()).unwrap();
                        sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = std::cmp::min(backoff_secs + backoff_secs, 300);
                        continue;
                    }
                    let res = res.unwrap();
                    if !res.status().is_success() {
                        eprintln!("HTTP request failed with status {}", res.status());
                        eprintln!("trying again in {} seconds.", backoff_secs);
                        tx.send(default_part.clone()).unwrap();
                        sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = std::cmp::min(backoff_secs + backoff_secs, 300);
                        continue;
                    }
                    println!("Reading {}", &stream.1.url);
                    let content_type: mime::Mime = res
                        .headers()
                        .get(CONTENT_TYPE)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let stream = res.bytes_stream();
                    let boundary = content_type.get_param(mime::BOUNDARY).unwrap();
                    let mut stream = multipart_stream::parse(stream, boundary.as_str());
                    assert_eq!(content_type.type_(), "multipart");
                    while let Some(p) = stream.next().await {
                        let p = p.unwrap();
                        tx.send(p.clone()).unwrap();
                        //println!("send {:?}", p.headers);
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }
}
