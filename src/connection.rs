use axum::body::Bytes;
use futures::Stream;
use lazy_static_include::lazy_static_include_bytes;
use multipart_stream::{parser::Error, Part};
use std::{
    pin::Pin,
    sync::{Arc, LazyLock, Mutex, Weak},
    task::Poll,
    time::Duration,
};
use tokio::{
    select,
    sync::watch,
    time::{interval, sleep},
};
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use tokio_util::sync::CancellationToken;

lazy_static_include_bytes! {
    DEFAULT_BODY_CONTENT => "res/no_feed_640x480.jpg",
}

static DEFAULT_PART: LazyLock<Part> = LazyLock::new(|| {
    let default_body = Bytes::from(&DEFAULT_BODY_CONTENT[..]);
    let mut headers = multipart_http::HeaderMap::new();
    headers.insert(
        multipart_http::header::CONTENT_TYPE,
        "image/jpeg".parse().unwrap(),
    );
    headers.insert(
        multipart_http::header::CONTENT_LENGTH,
        default_body.len().into(),
    );
    Part {
        headers,
        body: default_body,
    }
});

pub struct Connection {
    url: String,
    delay: Duration,
    sender: watch::Sender<Part>,
    cancellation_token: Mutex<Option<CancellationToken>>,
}

pub struct ConnectionReader<T: Stream<Item = Part>> {
    connection: Weak<Connection>,
    stream: Pin<Box<T>>,
}

impl Connection {
    pub fn new(url: &str, fps: u8) -> Self {
        // start server broadcast channel writer
        let (tx, _) = watch::channel(DEFAULT_PART.clone());

        Connection {
            url: url.to_string(),
            delay: Duration::from_micros(1000000 / fps as u64),
            sender: tx,
            cancellation_token: Mutex::new(None),
        }
    }

    pub fn subscribe(conn: Arc<Connection>) -> ConnectionReader<impl Stream<Item = Part>> {
        {
            let mut guard = conn.cancellation_token.lock().unwrap();
            if guard.is_none() {
                let s = conn.sender.clone();
                let ct = CancellationToken::new();
                tokio::spawn(Connection::read_camera(conn.url.clone(), s, ct.clone()));
                *guard = Some(ct);
            }
        }
        let reader = conn.sender.subscribe();
        ConnectionReader {
            stream: Box::pin(
                IntervalStream::new(interval(conn.delay.clone()))
                    .map(move |_| reader.borrow().clone()),
            ),
            connection: Arc::downgrade(&conn),
        }
    }

    async fn connect(url: &str) -> impl Stream<Item = Result<Part, Error>> {
        let mut backoff_secs = 1;
        loop {
            let res = reqwest::get(url).await;
            if res.is_err() {
                eprintln!("err: {:?}", res.err().unwrap());
                eprintln!("trying again in {} seconds.", backoff_secs);
                sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = std::cmp::min(backoff_secs + backoff_secs, 300);
                continue;
            }
            let res = res.unwrap();
            if !res.status().is_success() {
                eprintln!("HTTP request failed with status {}", res.status());
                eprintln!("trying again in {} seconds.", backoff_secs);
                sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = std::cmp::min(backoff_secs + backoff_secs, 300);
                continue;
            }
            println!("Reading {}", url);
            let content_type: mime::Mime = res
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(content_type.type_(), "multipart");
            let stream = res.bytes_stream();
            let boundary = content_type.get_param(mime::BOUNDARY).unwrap();
            return multipart_stream::parse(stream, boundary.as_str());
        }
    }

    async fn read_camera(
        url: String,
        sender: watch::Sender<Part>,
        cancellation_token: CancellationToken,
    ) {
        sender.send(DEFAULT_PART.clone()).unwrap();
        let mut stream;
        select! {
            biased;
            _ = cancellation_token.cancelled() => return,
            s = Connection::connect(&*url) => stream = s,
        };

        loop {
            let maybe = select! {
                biased;
                _ = cancellation_token.cancelled() => return,
                maybe = stream.next() => maybe,
            };
            if let Some(p) = maybe {
                let p = p.unwrap();
                sender.send(p).unwrap();
            } else {
                break;
            }
        }
    }

    fn on_dropped_connection(&self) {
        let mut guard = self.cancellation_token.lock().unwrap();
        if self.sender.receiver_count() <= 1 {
            guard.take().unwrap().cancel();
        }
    }
}

impl<T: Stream<Item = Part>> Drop for ConnectionReader<T> {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.upgrade() {
            conn.on_dropped_connection();
        }
    }
}

impl<T: Stream<Item = Part>> Stream for ConnectionReader<T> {
    type Item = T::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Pin::into_inner(self).stream.as_mut().poll_next(cx)
    }
}
