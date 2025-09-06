use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper::{Method, Request, Uri};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test URLs for various scenarios
    let test_urls = vec![
        "http://web-platform.test:8000/fetch/h1-parsing/resources/status-code.py?input=F00%20NotANumber",
        "http://web-platform.test:8000/fetch/h1-parsing/resources/status-code.py?input=600%20Over599",
        "http://web-platform.test:8000/fetch/h1-parsing/resources/status-code.py?input=99%20Under100",
    ];

    for url in test_urls {
        println!("\n{}", "=".repeat(60));
        println!("Testing URL: {}", url);
        println!("{}", "=".repeat(60));

        match request(url).await {
            Ok(response) => {
                println!("✅ Request successful!");
                println!("Status: {}", response.status());
                println!("Headers: {:#?}", response.headers());

                let body_bytes = response.into_body().collect().await?.to_bytes();
                let body_str = String::from_utf8_lossy(&body_bytes);
                let truncated_body = if body_str.len() > 500 {
                    format!(
                        "{}... [truncated, total {} chars]",
                        &body_str[..500],
                        body_str.len()
                    )
                } else {
                    body_str.to_string()
                };
                println!("Response body: {}", truncated_body);
            }
            Err(e) => {
                eprintln!("❌ Error occurred: {}", e);
                eprintln!("Error details: {:?}", e);
            }
        }
    }

    Ok(())
}

struct TokioIo<T> {
    inner: T,
}

impl<T> TokioIo<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: AsyncRead + Unpin> hyper::rt::Read for TokioIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let n = unsafe {
            let mut tokio_buf = tokio::io::ReadBuf::uninit(buf.as_mut());
            match AsyncRead::poll_read(Pin::new(&mut self.inner), cx, &mut tokio_buf) {
                Poll::Ready(Ok(())) => tokio_buf.filled().len(),
                other => return other,
            }
        };

        unsafe {
            buf.advance(n);
        }
        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncWrite + Unpin> hyper::rt::Write for TokioIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut self.inner), cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.inner), cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.inner), cx)
    }
}

async fn request(url: &str) -> Result<hyper::Response<Incoming>, Box<dyn std::error::Error>> {
    let uri: Uri = url.parse()?;
    let host = uri.host().unwrap_or("localhost");
    let port = uri.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    println!("Connecting to: {}", addr);
    let stream = TcpStream::connect(&addr).await?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            eprintln!("Connection failed: {}", err);
        }
    });

    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Empty::<hyper::body::Bytes>::new())?;

    let response = sender.send_request(req).await?;
    Ok(response)
}
