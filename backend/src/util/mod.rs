use std::collections::HashSet;
use crate::api::APIError;
use crate::util::amazon::{setup, USERS_TABLE_NAME};
use crate::util::database::{AccountLink, BeatMap, User};
use crate::SiteData;
use std::sync::{Arc, LockResult};
use firebase_auth::FirebaseAuth;
use tokio::sync::Mutex;
use uuid::Uuid;
use lazy_static::lazy_static;
use crate::util::ratelimiter::Ratelimiter;

pub mod amazon;
pub mod database;
pub mod ratelimiter;
pub mod warp;
pub mod data;
pub mod image;

static mut DATA: Option<SiteData> = None;
lazy_static! {
    static ref DATA_MUTEX: Mutex<()> = Mutex::new(());
}

pub async fn data() -> SiteData {
    unsafe {
        if DATA.is_none() {
            let _lock = match DATA_MUTEX.try_lock() {
                Ok(lock) => lock,
                Err(_) => {
                    let _ = DATA_MUTEX.lock().await;
                    return DATA.clone().unwrap();
                }
            };

            DATA = Some(SiteData {
                auth: FirebaseAuth::new("beatblockbrowser").await,
                amazon: setup().await.unwrap(),
                ratelimiter: Arc::new(std::sync::Mutex::new(Ratelimiter::new())),
            });
            return DATA.clone().unwrap();
        }
        DATA.clone().unwrap()
    }
}

pub async fn get_user_from_link(account_link: AccountLink) -> Result<User, APIError> {
    get_or_create_user(account_link.clone(), move || User {
        id: Uuid::new_v4(),
        links: vec![account_link.clone()],
        ..Default::default()
    })
    .await
}

pub async fn get_or_create_user<F: Fn() -> User>(
    account_link: AccountLink,
    default_user: F,
) -> Result<User, APIError> {
    if let Some(user) = data().await.amazon
        .query_by_link(account_link)
        .await
        .map_err(APIError::database_error)?
    {
        return Ok(user);
    }
    let user = default_user();
    data().await.amazon
        .upload(USERS_TABLE_NAME, &user, None::<&Vec<String>>)
        .await
        .map_err(APIError::database_error)?;
    Ok(user)
}

pub fn get_search_combos(song: &BeatMap) -> Vec<String> {
    let mut output = HashSet::new();
    add_word_combos(&song.song, &mut output);
    add_word_combos(&song.artist, &mut output);
    output.extend(song.charter.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty())
        .take(3).map(ToString::to_string));
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

pub trait LockResultExt {
    type Guard;

    /// Returns the lock guard even if the mutex is [poisoned].
    ///
    /// [poisoned]: https://doc.rust-lang.org/stable/std/sync/struct.Mutex.html#poisoning
    fn ignore_poison(self) -> Self::Guard;
}

impl<Guard> LockResultExt for LockResult<Guard> {
    type Guard = Guard;

    fn ignore_poison(self) -> Guard {
        self.unwrap_or_else(|e| e.into_inner())
    }
}
