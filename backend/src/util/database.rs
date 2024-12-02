use crate::parsing::LevelVariant;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type UserID = Uuid;
pub type MapID = Uuid;

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

impl AccountLink {
    pub fn id(&self) -> String {
        match self {
            AccountLink::Discord(id) => id.to_string(),
            AccountLink::Google(id) => id.clone()
        }
    }
}