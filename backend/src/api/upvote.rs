use crate::api::APIError;
use crate::util::amazon::{MAPS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::database::{BeatMap, User};
use aws_sdk_dynamodb::types::AttributeValue;
use warp::{Rejection, Reply};
use crate::util::data;
use crate::util::warp::Replyable;

pub async fn upvote(
    user: User,
    map: BeatMap
) -> Result<impl Reply, Rejection> {
    upvote_for_map(&map, &user).await?;
    Ok("Ok".reply())
}

pub async fn upvote_for_map(map: &BeatMap, user: &User) -> Result<(), APIError> {
    if user.upvoted.contains(&map.id) {
        return Err(APIError::AlreadyUpvoted());
    }

    data().await.amazon
        .update(MAPS_TABLE_NAME, map.id.to_string(), |builder| {
            builder
                .update_expression("SET upvotes = if_not_exists(upvotes, :start) + :inc")
                .expression_attribute_values(":start", AttributeValue::N("0".to_string()))
                .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
        })
        .await.map_err(APIError::database_error)?;
    data().await.amazon.add_to_list(USERS_TABLE_NAME, user.id.to_string(), "upvoted", map.id.to_string()).await?;
    Ok(())
}

pub async fn unvote(
    mut user: User,
    map: BeatMap
) -> Result<impl Reply, Rejection> {
    unvote_for_map(&map, &mut user).await?;
    Ok("Ok".reply())
}

pub async fn unvote_for_map(map: &BeatMap, user: &mut User) -> Result<(), APIError> {
    user.upvoted.remove(user.upvoted.iter().position(|elem| elem == &map.id).ok_or(APIError::AlreadyUpvoted())?);

    data().await.amazon
        .update(MAPS_TABLE_NAME, map.id.to_string(), |builder| {
            builder
                .update_expression("SET upvotes = :start - :dec")
                .expression_attribute_values(":start", AttributeValue::N(map.upvotes.to_string()))
                .expression_attribute_values(":dec", AttributeValue::N("1".to_string()))
        })
        .await.map_err(APIError::database_error)?;
    data().await.amazon.overwrite_list(USERS_TABLE_NAME, user.id.to_string(), "upvoted", user.upvoted.clone()).await?;
    Ok(())
}
