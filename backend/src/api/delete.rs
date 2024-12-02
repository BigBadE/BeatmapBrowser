use warp::{Rejection, Reply};
use crate::api::APIError;
use crate::util::amazon::{MAPS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::data;
use crate::util::database::{BeatMap, User};
use crate::util::warp::Replyable;

pub const ADMINS: [&'static str; 1] = ["gfde6dkqtey5trmfya8h"];

pub async fn delete(
    mut user: User,
    map: BeatMap
) -> Result<impl Reply, Rejection> {
    if map.charter_uid != user.id && !ADMINS.contains(&user.id.to_string().as_str()) {
        return Err(APIError::PermissionError().into());
    }
    
    user.maps.remove(user.maps.iter().position(|elem| elem == &map.id).ok_or(APIError::AlreadyDownloaded())?);
    data().await.amazon.remove(MAPS_TABLE_NAME, "id", map.id.to_string()).await
        .map_err(APIError::database_error)?;
    data().await.amazon.overwrite_list(USERS_TABLE_NAME, user.id.to_string(), "maps", user.maps).await?;
    data().await.amazon.delete_object(format!("{}.zip", map.id).as_str()).await.map_err(APIError::database_error)?;
    data().await.amazon.delete_object(format!("{}.png", map.id).as_str()).await.map_err(APIError::database_error)?;
    Ok("Ok".reply())
}
