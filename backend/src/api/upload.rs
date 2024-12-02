use crate::api::APIError;
use crate::parsing::{check_archive, get_parser, parse_archive, BackgroundData};
use crate::util::amazon::{MAPS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::database::{BeatMap, UserID};
use crate::util::image::save_image;
use crate::util::ratelimiter::{SiteAction, UniqueIdentifier};
use crate::util::warp::{get_user, Replyable};
use crate::util::{data, LockResultExt};
use aws_sdk_dynamodb::types::AttributeValue;
use bytes::BufMut;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use std::time::{Duration, SystemTime};
use tokio::time::timeout;
use uuid::Uuid;
use warp::multipart::FormData;
use warp::{Rejection, Reply};

pub const MAX_SIZE: u32 = 200000000;

#[derive(Default, Serialize, Deserialize)]
pub struct UploadForm {
    #[serde(rename = "firebaseToken")]
    firebase_token: String,
    beatmap: Vec<u8>,
}

pub async fn upload(identifier: UniqueIdentifier, form: FormData) -> Result<impl Reply, Rejection> {
    let form: Vec<(String, Vec<u8>)> = form.and_then(|mut field| async move {
        let mut buffer = Vec::new();
        while let Some(data) = field.data().await {
            buffer.put(data?);
        }
        Ok((field.name().to_string(), buffer))
    })
        .try_collect()
        .await.map_err(|_| APIError::ArgumentError())?;
    
    let mut beatmap = None;
    let mut token = None;
    for (name, data) in form {
        match name.as_str() {
            "beatmap" => beatmap = Some(data),
            "firebaseToken" => token = Some(data),
            _ => return Err(APIError::ArgumentError().into()),
        }
    }
    
    let user = get_user(String::from_utf8_lossy(token.ok_or(APIError::ArgumentError())?.deref()).to_string()).await?;
    let map = timeout(
        Duration::from_millis(10000),
        upload_beatmap(
            beatmap.ok_or(APIError::ArgumentError())?,
            identifier,
            user.id,
        ),
    )
    .await.map_err(|err| APIError::TimeoutError(err))??;
    
    Ok(format!("query={} {}", map.charter, map.song).reply())
}

pub async fn upload_beatmap(
    mut beatmap_data: Vec<u8>,
    ip: UniqueIdentifier,
    charter_id: UserID,
) -> Result<BeatMap, APIError> {
    let (mut beatmap, image, bg_data) = create_beatmap(&mut beatmap_data, charter_id)?;

    // Save the beatmap
    if let Some(map) = data().await
        .amazon
        .query(MAPS_TABLE_NAME, "charter_uid", charter_id.to_string())
        .await
        .map_err(APIError::database_error)?
        .into_iter()
        .filter(|map: &BeatMap| map.song == beatmap.song)
        .next()
    {
        // Update the old map instead
        beatmap.id = map.id;
        data().await.amazon
            .update(MAPS_TABLE_NAME, beatmap.id.to_string(), |builder| {
                builder
                    .update_expression("SET upload_date = :date")
                    .expression_attribute_values(
                        ":date",
                        AttributeValue::S(<DateTime<Utc> as ToString>::to_string(&DateTime::from(
                            SystemTime::now(),
                        ))),
                    )
            })
            .await
            .map_err(APIError::database_error)?;
    } else {
        data().await.ratelimiter
            .lock()
            .ignore_poison()
            .check_limited(SiteAction::Upload, &ip)?;

        data().await.amazon
            .add_to_list(
                USERS_TABLE_NAME,
                charter_id.to_string(),
                "maps",
                beatmap.id.to_string(),
            )
            .await?;
        data().await.amazon
            .upload_song(&beatmap)
            .await
            .map_err(APIError::database_error)?;
    }

    save_image(&image, &bg_data, &&beatmap.id).await?;
    data().await.amazon
        .upload_object(beatmap_data, format!("{}.zip", beatmap.id).as_str())
        .await
        .map_err(APIError::database_error)?;
    Ok(beatmap)
}

pub fn create_beatmap(
    beatmap: &mut Vec<u8>,
    charter_id: UserID,
) -> Result<(BeatMap, Option<Vec<u8>>, Option<BackgroundData>), APIError> {
    let file_data = parse_archive(get_parser(beatmap)?.deref_mut()).map_err(APIError::ZipError)?;
    check_archive(beatmap).map_err(APIError::ZipError)?;

    Ok((
        BeatMap {
            song: file_data.level_data.song_name,
            artist: file_data.level_data.artist,
            charter: file_data.level_data.charter,
            difficulties: file_data
                .level_data
                .difficulty
                .map_or(file_data.level_data.variants, |diff| vec![diff.into()]),
            description: file_data.level_data.description,
            artist_list: file_data.level_data.artist_list,
            charter_uid: charter_id,
            image: file_data.image.is_some(),
            upvotes: 0,
            upload_date: DateTime::from(SystemTime::now()),
            update_date: DateTime::from(SystemTime::now()),
            id: Uuid::new_v4(),
        },
        file_data.image,
        file_data.level_data.bg_data,
    ))
}
