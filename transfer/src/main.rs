use crate::amazon::{setup, MAPS_TABLE_NAME, USERS_TABLE_NAME};
use crate::surreal::connect;
use crate::types::{AccountLink, BeatMap, SurrealBeatMap, SurrealUser, User};
use anyhow::Error;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::str::FromStr;
use surrealdb::sql::Thing;
use uuid::Uuid;

mod amazon;
mod surreal;
mod types;

#[tokio::main]
pub async fn main() -> Result<(), Error> {
    let surreal = connect().await?;
    let amazon = setup().await?;

    let mut new_ids = HashMap::new();
    let maps: Vec<SurrealBeatMap> = surreal.select("beatmaps").await?;
    println!("Ready");
    for map in maps {
        let new_id = to_uuid(map.id.as_ref().unwrap());
        let charter_uid = new_ids
            .entry(
                map.charter_uid
                    .as_ref()
                    .unwrap()
                    .split(":")
                    .last()
                    .unwrap()
                    .to_string(),
            )
            .or_insert(Uuid::new_v4())
            .clone();

        println!("Uploading {} for {}", new_id, charter_uid);
        let mut new_map = BeatMap {
            song: map.song,
            artist: map.artist,
            charter: map.charter,
            charter_uid,
            difficulties: map.difficulties,
            description: map.description,
            artist_list: map.artist_list,
            image: map.image.is_some(),
            upvotes: map.upvotes,
            upload_date: map.upload_date,
            update_date: map.update_date,
            id: new_id.clone(),
            title_prefix: vec![],
        };
        new_map.title_prefix = get_search_combos(&new_map);
        amazon.upload(MAPS_TABLE_NAME, &new_map).await?;
        amazon
            .upload_object(
                fs::read(format!("site/output/{}", map.download))?,
                format!("{}.zip", new_id.clone()).as_str(),
            )
            .await?;
        if let Some(image) = map.image {
            amazon
                .upload_object(
                    fs::read(format!("site/output/{}", image))?,
                    format!("{}.png", new_id.clone()).as_str(),
                )
                .await?;
        }
    }
    let users: Vec<SurrealUser> = surreal.select("users").await?;
    for user in users {
        let new_id = user.id.as_ref().unwrap().id.to_string();
        let id = new_ids.entry(new_id).or_insert(Uuid::new_v4()).clone();
        let mut new_user = User {
            maps: user.maps.iter().map(to_uuid).collect(),
            downloaded: user.downloaded.iter().map(to_uuid).collect(),
            upvoted: user.upvoted.iter().map(to_uuid).collect(),
            id,
            links: vec![],
        };
        if let Some(google) = user.google_id {
            new_user.links.push(AccountLink::Google(google));
        }

        if let Some(discord) = user.discord_id {
            new_user.links.push(AccountLink::Discord(discord));
        }
        amazon.upload(USERS_TABLE_NAME, &new_user).await?;
    }
    Ok(())
}

pub fn get_search_combos(song: &BeatMap) -> Vec<String> {
    let mut output = HashSet::new();
    add_word_combos(&song.song, &mut output);
    add_word_combos(&song.charter, &mut output);
    add_word_combos(&song.artist, &mut output);
    output.into_iter().collect()
}

pub fn add_word_combos(word: &String, output: &mut HashSet<String>) {
    let word = word.to_lowercase();
    output.extend(
        word.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty())
            .take(3)
            .flat_map(|word| {
                let mut folded =
                    word.chars()
                        .take(10)
                        .fold(vec![], |mut acc, c| {
                            if acc.is_empty() {
                                acc.push(c.to_string());
                            } else {
                                acc.push(format!("{}{}", acc.last().unwrap(), c));
                            }
                            acc
                        })
                        .into_iter()
                        .skip(word.len().max(4).min(9) - 4)
                        .collect::<Vec<_>>();
                folded.push(word.to_string());
                folded
            }),
    );
    output.insert(word.chars().filter(|c| !c.is_alphanumeric()).collect());
}

pub fn to_uuid(thing: &Thing) -> Uuid {
    let new_id = thing.id.to_string();
    Uuid::from_str(&new_id[3..new_id.len() - 3]).unwrap()
}
