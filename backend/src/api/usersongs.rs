use crate::api::APIError;
use crate::util::amazon::{MAPS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::database::{BeatMap, User};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use warp::{Rejection, Reply};
use crate::util::data;
use crate::util::warp::Replyable;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserpageArguments {
    pub user: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SongsResult {
    pub results: Vec<BeatMap>,
}

pub async fn usersongs(
    user: String
) -> Result<impl Reply, Rejection> {
    let user: User = data().await.amazon.query_one(USERS_TABLE_NAME, "id", user)
        .await
        .map_err(|err| APIError::DatabaseError(err.into()))?
        .ok_or(APIError::KnownArgumentError(Error::msg("No user with that id")))?;
    
    let mut maps: Vec<BeatMap> = data().await.amazon.query(MAPS_TABLE_NAME, "charter_uid", user.id.to_string())
        .await
        .map_err(|err| APIError::DatabaseError(err.into()))?;
    maps.sort_by(|first, second| first.upvotes.cmp(&second.upvotes).reverse());
    Ok(SongsResult {
        results: maps,
    }.reply())
}
