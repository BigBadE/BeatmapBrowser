use crate::api::APIError;
use crate::util::amazon::{MAPS_TABLE_NAME, TOKENS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::database::{BeatMap, User};
use crate::util::ratelimiter::{SiteAction, UniqueIdentifier};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use warp::body::json;
use warp::reject::Reject;
use warp::{reject, reply, Filter, Rejection, Reply};
use crate::api::signin::UserToken;
use crate::util::data;

pub fn extract_identifier() -> impl Filter<Extract = (UniqueIdentifier,), Error = Infallible> + Copy {
    warp::addr::remote()
        .map(|addr: Option<SocketAddr>| match addr.unwrap_or("0.0.0.0:0".parse().unwrap()) {
            SocketAddr::V4(ip) => UniqueIdentifier::Ipv4(ip.ip().clone()),
            SocketAddr::V6(ip) => UniqueIdentifier::Ipv6(ip.ip().clone()),
        })
}

pub fn check_ratelimit(action: SiteAction) -> impl Filter<Extract = ((),), Error = Rejection> + Copy {
    extract_identifier().and_then(move |identifier: UniqueIdentifier| async move {
        data().await.ratelimiter.lock().unwrap().check_limited(action, &identifier).map_err(reject::custom)
    })
}

pub fn extract_map() -> impl Filter<Extract = ((User, BeatMap),), Error = Rejection> + Copy {
    json::<MapRequest>()
        .and_then(|request: MapRequest| async move {
            Ok::<(User, BeatMap), Rejection>((get_user(request.token).await.map_err(reject::custom)?, get_map(request.map_id).await.map_err(reject::custom)?))
        })
}

pub async fn handle_error(err: Rejection) -> Result<impl Reply, Rejection> {
    if let Some(api_error) = err.find::<APIError>() {
        match api_error {
            APIError::Ratelimited() => {},
            err => println!("Error: {:?}", err)
        }
        let response = reply::json(&serde_json::json!({
            "error": api_error.to_string()
        }));
        return Ok(reply::with_status(response, api_error.get_code()));
    }
    println!("Denied connection: {:?}", err);
    Err(err)
}

pub fn handle_auth() -> impl Filter<Extract = (User,), Error = Rejection> + Copy {
    json::<AuthenticatedRequest>()
        .and_then(|request: AuthenticatedRequest| async move {
            get_user(request.token).await.map_err(reject::custom)
        })
}

pub async fn get_user(token: String) -> Result<User, APIError> {
    let user_id: UserToken = data().await.amazon.query_one(TOKENS_TABLE_NAME, "user_token", token)
        .await
        .map_err(APIError::database_error)?
        .ok_or(APIError::AuthError("Invalid token!".to_string()))?;
    data().await.amazon.query_one(USERS_TABLE_NAME, "id", user_id.id.to_string())
        .await
        .map_err(APIError::database_error)?
        .ok_or(APIError::AuthError("Invalid token!".to_string()))
}

async fn get_map(id: String) -> Result<BeatMap, APIError> {
    data().await.amazon.query_one(MAPS_TABLE_NAME, "id", id)
        .await
        .map_err(APIError::database_error)?
        .ok_or(APIError::AuthError("Invalid map!".to_string()))
}

pub trait Replyable {
    fn reply(self) -> impl Reply;
}

impl<T: Serialize> Replyable for T {
    fn reply(self) -> impl Reply {
        reply::json(&self)
    }
}

impl Reject for APIError {}

#[derive(Debug, Deserialize)]
pub struct AuthenticatedRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct MapRequest {
    #[serde(rename = "mapId")]
    pub map_id: String,
    token: String,
}