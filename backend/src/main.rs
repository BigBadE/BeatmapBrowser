mod api;
mod discord;
mod parsing;
mod util;

use crate::api::delete::delete;
use crate::api::downloaded::{download, remove};
use crate::api::search::search;
use crate::api::signin::{discord_signin, discord_sync, google_signin, google_sync};
use crate::api::upload::upload;
use crate::api::upvote::{unvote, upvote};
use crate::api::usersongs::usersongs;
use crate::discord::run_bot;
use crate::util::amazon::Amazon;
use crate::util::database::User;
use crate::util::ratelimiter::{Ratelimiter, SiteAction};
use crate::util::warp::{check_ratelimit, extract_identifier, extract_map, handle_auth, handle_error, Replyable};
use firebase_auth::FirebaseAuth;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use warp::path::param;
use warp::{get, multipart, path, post, Filter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting version {}", env!("CARGO_PKG_VERSION"));

    let _site = std::env::args().nth(2).unwrap();

    let _ = tokio::spawn(run_bot());

    let limit = |action, in_path: &'static str| path("api").and(path(in_path)).and(check_ratelimit(action)).untuple_one();
    let limit_param = |action, path| limit(action, path).and(get()).and(param::<String>());
    let auth = |action, path| limit(action, path).and(post()).and(handle_auth());
    let auth_map = |action, path| limit(action, path).and(post()).and(extract_map()).untuple_one();

    warp::serve(
        auth(SiteAction::UpvoteList, "account_data")
            .map(|user: User| user.reply())
            .or(auth_map(SiteAction::Search, "delete").and_then(delete))
            .or(auth_map(SiteAction::Download, "download").and_then(download))
            .or(auth_map(SiteAction::Download, "remove").and_then(remove))
            .or(limit_param(SiteAction::Search, "search").and_then(search))
            .or(auth_map(SiteAction::Search, "upvote").and_then(upvote))
            .or(auth_map(SiteAction::Search, "unvote").and_then(unvote))
            .or(limit(SiteAction::Search, "upload")
                .and(post())
                .and(extract_identifier())
                .and(multipart::form())
                .and_then(upload))
            .or(limit_param(SiteAction::Search, "usersongs").and_then(usersongs))
            .or(limit_param(SiteAction::UpvoteList, "discordauth").and_then(discord_signin))
            .or(auth(SiteAction::UpvoteList, "discordsync").and(param()).and_then(discord_sync))
            .or(limit_param(SiteAction::UpvoteList, "googleauth").and_then(google_signin))
            .or(auth(SiteAction::UpvoteList, "googlesync").and(param()).and_then(google_sync))
            .or(warp::fs::dir(std::env::args().nth(2).unwrap()))
            .recover(handle_error),
    )
    .run(std::env::args().nth(1).unwrap().parse::<SocketAddr>().unwrap())
    .await;
    Ok(())
}

#[derive(Clone)]
pub struct SiteData {
    auth: FirebaseAuth,
    amazon: Amazon,
    ratelimiter: Arc<Mutex<Ratelimiter>>,
}
