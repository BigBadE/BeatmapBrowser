use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;
use uuid::Uuid;

pub type UserID = Uuid;
pub type MapID = Uuid;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct SurrealBeatMap {
    pub song: String,
    pub artist: String,
    pub charter: String,
    pub charter_uid: Option<String>,
    pub difficulties: Vec<LevelVariant>,
    pub description: String,
    pub artist_list: String,
    pub image: Option<String>,
    pub download: String,
    pub upvotes: u64,
    pub upload_date: DateTime<Utc>,
    pub update_date: DateTime<Utc>,
    pub id: Option<Thing>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SurrealUser {
    pub maps: Vec<Thing>,
    pub downloaded: Vec<Thing>,
    pub upvoted: Vec<Thing>,
    pub id: Option<Thing>,
    pub discord_id: Option<u64>,
    pub google_id: Option<String>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BeatMap {
    pub song: String,
    pub artist: String,
    pub charter: String,
    pub charter_uid: UserID,
    pub difficulties: Vec<LevelVariant>,
    pub description: String,
    pub artist_list: String,
    pub image: bool,
    pub upvotes: u64,
    pub upload_date: DateTime<Utc>,
    pub update_date: DateTime<Utc>,
    pub id: MapID,
    pub title_prefix: Vec<String>
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub maps: Vec<MapID>,
    pub downloaded: Vec<MapID>,
    pub upvoted: Vec<MapID>,
    pub id: UserID,
    pub links: Vec<AccountLink>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountLink {
    #[serde(rename = "discord")]
    Discord(u64),
    #[serde(rename = "google")]
    Google(String)
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LevelVariant {
    display: String,
    difficulty: f64,
}