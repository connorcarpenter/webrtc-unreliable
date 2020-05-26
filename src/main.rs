use clap::{App, Arg};

use std::{
    net::{ IpAddr, SocketAddr }
};

use hyper::{
    header::{self, HeaderValue},
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Error, Method, Response, Server, StatusCode,
};
use log::{info, warn};

use webrtc_unreliable::{Server as RtcServer, MAX_MESSAGE_LEN, MessageType};

use env_logger;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let address: &str = "192.168.1.6:3171";
    let session_listen_addr: SocketAddr = address
        .parse()
        .expect("could not parse HTTP address/port");
    let webrtc_listen_ip: IpAddr = session_listen_addr.ip();
    let webrtc_listen_addr = SocketAddr::new(webrtc_listen_ip, 3171);

    let mut rtc_server = RtcServer::new(webrtc_listen_addr, webrtc_listen_addr)
        .await
        .expect("could not start RTC server");

    let session_endpoint = rtc_server.session_endpoint();
    let make_svc = make_service_fn(move |addr_stream: &AddrStream| {
        let session_endpoint = session_endpoint.clone();
        let remote_addr = addr_stream.remote_addr();
        async move {
            Ok::<_, Error>(service_fn(move |req| {
                let mut session_endpoint = session_endpoint.clone();
                async move {
                    if req.uri().path() == "/"
                        || req.uri().path() == "/index.html" && req.method() == Method::GET
                    {
                        info!("serving example index HTML to {}", remote_addr);
                        Response::builder().body(Body::from(include_str!("echo_server.html")))
                    } else if req.uri().path() == "/new_rtc_session" && req.method() == Method::POST
                    {
                        info!("WebRTC session request from {}", remote_addr);
                        match session_endpoint.http_session_request(req.into_body()).await {
                            Ok(mut resp) => {
                                resp.headers_mut().insert(
                                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                                    HeaderValue::from_static("*"),
                                );
                                Ok(resp.map(Body::from))
                            }
                            Err(err) => Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::from(format!("error: {}", err))),
                        }
                    } else {
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::from("not found"))
                    }
                }
            }))
        }
    });

    tokio::spawn(async move {
        Server::bind(&session_listen_addr)
            .serve(make_svc)
            .await
            .expect("HTTP session server has died");
    });

    let mut message_buf = vec![0; MAX_MESSAGE_LEN];
    loop {
        match rtc_server.recv(&mut message_buf).await {
            Ok(received) => {
                if let Err(err) = rtc_server
                    .send(
                        &[4, 8, 15, 16, 23, 42],
                        MessageType::Binary,
                        &received.remote_addr,
                    )
                    .await
                {
                    warn!(
                        "could not send message to {}: {}",
                        received.remote_addr, err
                    )
                }
            }
            Err(err) => warn!("could not receive RTC message: {}", err),
        }
    }
}
