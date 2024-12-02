use std::ops::Deref;
use crate::api::APIError;
use crate::util::database::BeatMap;
use serde::{Deserialize, Serialize};
use urlencoding::decode;
use warp::{Rejection, Reply};
use crate::util::data;
use crate::util::warp::Replyable;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub results: Vec<BeatMap>,
}

pub async fn search(
    query: String
) -> Result<impl Reply, Rejection> {
    let query = decode(query.deref()).map_err(|_| APIError::ArgumentError())?.to_string();
    Ok(SearchResult {
        query: query.clone(),
        results: data().await.amazon.search_songs(&query).await.map_err(APIError::database_error)?,
    }.reply())
}