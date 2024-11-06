mod parsing;
mod discord;
mod api;
mod util;

use crate::api::search::search_database;
use crate::api::upload::upload;
use crate::api::upvote::{upvote, upvote_list};
use crate::discord::run_bot;
use crate::util::body::EitherBody;
use crate::util::database::connect;
use crate::util::ratelimiter::{Ratelimiter, UniqueIdentifier};
use crate::util::to_weberr;
use anyhow::{Context, Error};
use firebase_auth::FirebaseAuth;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::http::response::Builder;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_staticfile::Static;
use hyper_util::rt::{TokioIo, TokioTimer};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use surrealdb::engine::remote::ws::Client;
use surrealdb::Surreal;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct TokioExecutor;

impl<F> hyper::rt::Executor<F> for TokioExecutor
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn(fut);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = std::env::args().nth(1).unwrap().parse().unwrap();

    let data = SiteData {
        site: Static::new(Path::new("site/")),
        db: connect().await?,
        auth: FirebaseAuth::new("beatblockbrowser").await,
        ratelimiter: Arc::new(Mutex::new(Ratelimiter::new())),
    };

    let _ = tokio::spawn(run_bot(data.db.clone(), data.ratelimiter.clone()));

    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to create TCP listener");
    eprintln!("Server running on http://{}/", addr);
    loop {
        match handle_connection(&listener, data.clone()).await {
            Ok(()) => {}
            Err(err) => println!("Error serving connection: {err:?}")
        }
    }
}

async fn handle_connection(listener: &TcpListener, data: SiteData) -> Result<(), Error> {
    let (stream, ip) = listener
        .accept()
        .await
        .expect("Failed to accept TCP connection");

    tokio::spawn(async move {
        if let Err(err) = http1::Builder::new()
            .timer(TokioTimer::new())
            .serve_connection(
                TokioIo::new(stream),
                service_fn(move |req| {
                    let data = data.clone();
                    handle_request(req, ip, data)
                }),
            )
            .await
        {
            eprintln!("Error serving connection: {:?}", err);
        }
    });
    Ok(())
}

fn build_request(data: (StatusCode, String)) -> Result<Response<EitherBody>, hyper::http::Error> {
    Builder::new().status(data.0)
        .body(Full::new(Bytes::from(data.1)).into())
}

#[derive(Clone)]
pub struct SiteData {
    site: Static,
    db: Surreal<Client>,
    auth: FirebaseAuth,
    ratelimiter: Arc<Mutex<Ratelimiter>>,
}

async fn handle_request(request: Request<hyper::body::Incoming>, ip: SocketAddr, data: SiteData) -> Result<Response<EitherBody>, Error> {
    let identifier = match ip {
        SocketAddr::V4(ip) => UniqueIdentifier::Ipv4(ip.ip().clone()),
        SocketAddr::V6(ip) => UniqueIdentifier::Ipv6(ip.ip().clone())
    };
    let request_path = request.uri().path().to_string();
    let method = match (request.method(), &*request_path) {
        (&Method::GET, "/api/search") => to_weberr(search_database(request, identifier, &data).await),
        (&Method::POST, "/api/upvote_list") => to_weberr(upvote_list(request, identifier, &data).await),
        (&Method::POST, "/api/upload") => to_weberr(upload(request, identifier, &data).await),
        (&Method::POST, "/api/upvote") => to_weberr(upvote(request, identifier, &data).await),
        _ => return Ok(data.site.serve(request).await.context("Failed to serve static file")?.map(|body| body.into()))
    };

    Ok(match method {
        Ok(query) => Builder::new().status(StatusCode::OK).body(Full::new(Bytes::from(format!("{query}"))).into()),
        Err(error) => {
            println!("Error with {}: {:?}", request_path, error);
            build_request((error.get_code(), error.to_string()))
        }
    }?)
}