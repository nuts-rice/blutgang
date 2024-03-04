use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};

use crate::{
    log_info,
};

use http_body_util::Full;
use hyper_util_blutgang::rt::TokioIo;
use tokio::{
    net::TcpListener,
    sync::watch,
};

use hyper::{
    server::conn::http1,
    service::service_fn,
    body::Bytes,
    Request,
};

#[derive(Debug, PartialEq)]
enum ReadinessState {
    Ready,
    Setup,
}

#[macro_use]
macro_rules! readiness {
    (
        $io:expr,
        $readiness_rx:expr,
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_readiness_request(
                        req,
                        Arc::clone($readiness_rx),
                    );
                    response
                }),
            )
        {
            println!("error serving admin connection: {:?}", err);
        }
    };
}

fn accept_readiness_request(
    tx: Request<hyper::body::Incoming>,
    readiness_rx: Arc<watch::Receiver<ReadinessState>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    if *readiness_rx.borrow() == ReadinessState::Ready {
        return Ok(hyper::Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("ready")))
            .unwrap());
    } else {
        return Ok(hyper::Response::builder()
            .status(503)
            .body(Full::new(Bytes::from("starting up")))
            .unwrap());
    }
}

async fn readiness_server (
    readiness_rx: watch::Receiver<ReadinessState>,
    address: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a listener and bind to it
    let listener = TcpListener::bind(address).await?;
    log_info!("Bound admin API to: {}", address);
    let readiness_rx = Arc::new(readiness_rx);

    loop {
        let (stream, socketaddr) = listener.accept().await?;
        log_info!("Admin connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            readiness!(
                io,
                &readiness_rx,
            );
        });
    }
}