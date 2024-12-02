use warp::{Rejection, Reply};
use crate::api::APIError;
use crate::util::amazon::USERS_TABLE_NAME;
use crate::util::data;
use crate::util::database::{BeatMap, User};
use crate::util::warp::Replyable;

pub async fn download(
    user: User,
    map: BeatMap
) -> Result<impl Reply, Rejection> {
    if user.downloaded.contains(&map.id) {
        return Err(APIError::AlreadyDownloaded().into());
    }
    data().await.amazon.add_to_list(USERS_TABLE_NAME, user.id.to_string(), "downloaded", map.id.to_string()).await?;
    Ok("Ok".reply())
}

pub async fn remove(
    mut user: User,
    map: BeatMap
) -> Result<impl Reply, Rejection> {
    user.downloaded.remove(user.downloaded.iter().position(|elem| elem == &map.id).ok_or(APIError::AlreadyUpvoted())?);
    data().await.amazon.overwrite_list(USERS_TABLE_NAME, user.id.to_string(), "downloaded", user.downloaded).await?;
    Ok("Ok".reply())
}
