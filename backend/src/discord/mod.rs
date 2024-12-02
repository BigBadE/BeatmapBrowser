mod backlogger;

use crate::api::upload::{upload_beatmap, MAX_SIZE};
use crate::api::upvote::upvote_for_map;
use crate::api::APIError;
use crate::discord::backlogger::update_backlog;
use crate::util::database::AccountLink;
use crate::util::ratelimiter::UniqueIdentifier;
use crate::util::get_user_from_link;
use anyhow::Error;
use serenity::all::{CreateMessage, CreateThread, Http, ReactionType, Ready, UserId};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::prelude::*;
use serenity::small_fixed_array::FixedString;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use urlencoding::encode;

// Real server
#[cfg(not(debug_assertions))]
pub const WHITELISTED_GUILDS: [u64; 1] = [1277438162641223740];
#[cfg(not(debug_assertions))]
pub const WHITELISTED_CHANNELS: [u64; 1] = [1277438949870276661];

#[cfg(debug_assertions)]
pub const WHITELISTED_GUILDS: [u64; 0] = [];
#[cfg(debug_assertions)]
pub const WHITELISTED_CHANNELS: [u64; 1] = [1298415906388574279];

#[derive(Clone)]
struct Handler {
    
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, context: Context, message: Message) {
        if let Some(parent) = message
            .channel_id
            .to_channel(&context.http, None)
            .await
            .unwrap()
            .guild()
            .unwrap()
            .parent_id
        {
            if !WHITELISTED_CHANNELS.contains(&message.channel_id.into()) && !WHITELISTED_CHANNELS.contains(&parent.into()) {
                return;
            }
        } else if !WHITELISTED_CHANNELS.contains(&message.channel_id.into()) {
            return;
        }

        if let Err(error) = self.handle_message(&context.http, message, &HashSet::default())
            .await {
            println!("Error handling message: {error:?}");
        }
    }

    async fn ready(&self, ctx: Context, _data_about_bot: Ready) {
        if let Err(error) = update_backlog(self, &ctx).await {
            println!("Fatal error updating backlog: {error:?}");
        }
    }
}

impl Handler {
    pub async fn handle_message_dropped(
        &self,
        http: &Arc<Http>,
        message: Message,
        upvotes: &HashSet<UserId>,
    ) -> bool {
        self.handle_message(http, message, upvotes).await.unwrap_or(false)
    }

    pub async fn handle_message(
        &self,
        http: &Arc<Http>,
        message: Message,
        upvotes: &HashSet<UserId>,
    ) -> Result<bool, Error> {
        let mut found = false;
        for attachment in &message.attachments {
            if !attachment.filename.ends_with(".zip") && !attachment.filename.ends_with(".rar") {
                continue;
            }

            if attachment.size > MAX_SIZE {
                println!(
                    "Skipped massive file {}: {}",
                    attachment.filename, attachment.url
                );
                send_response(&http, &message, &"Failed to upload file! Size over 20MB limit!").await?;
                continue;
            }

            let file = attachment.download().await;
            match timeout(
                Duration::from_millis(5000),
                self.upload_map(file, message.author.id.into(), upvotes.clone()),
            )
                .await
            {
                Ok(result) => match result {
                    Ok(link) => {
                        found = true;
                        let channel = CreateThread::new(link.clone()).execute(http, message.channel_id, Some(message.id)).await?;
                        CreateMessage::new().content(format!("Map uploaded! Try it at https://beatblockbrowser.me/search.html?query={}", encode(&*link)))
                            .execute(http, channel.id, Some(channel.guild_id)).await?;
                        
                        if let Err(why) = message.react(&http, ReactionType::Unicode(FixedString::from_static_trunc("✔️"))).await {
                            println!("Error sending message: {why:?}");
                        }
                        /*if let Err(why) = message.react(&http, ReactionType::Unicode("🔼".to_string())).await {
                            println!("Error sending message: {why:?}");
                        }
                        if let Err(why) = message.react(&http, ReactionType::Unicode("🔽".to_string())).await {
                            println!("Error sending message: {why:?}");
                        }*/
                    }
                    Err(err) => {
                        let channel = message.author.create_dm_channel(http).await?;
                        CreateMessage::new().content(format!("Failed to upload beatmap! Error: {err}"))
                            .execute(http, channel.id, None).await?;
                        println!(
                            "Upload error for {} ({}): {err:?}",
                            message.link(),
                            attachment.url
                        );
                    }
                },
                Err(_) => {
                    let channel = message.author.create_dm_channel(http).await?;
                    CreateMessage::new().content("Failed to read the zip file! Please report this for it to sync properly")
                        .execute(http, channel.id, None).await?;
                    println!("Timeout error for {}", message.link());
                }
            }
        }
        Ok(found)
    }

    pub async fn upload_map(
        &self,
        file: Result<Vec<u8>, serenity::Error>,
        user_id: u64,
        upvotes: HashSet<UserId>,
    ) -> Result<String, APIError> {
        let user = get_user_from_link(AccountLink::Discord(user_id)).await?;
        let map = upload_beatmap(
            file?,
            UniqueIdentifier::Discord(user_id),
            user.id,
        )
            .await?;
        if map.upvotes == 0 {
            for user in &upvotes {
                let user = get_user_from_link(AccountLink::Discord(user.get())).await?;
                upvote_for_map(&map, &user).await?;
            }
        }
        Ok(format!("{} {}", map.charter, map.song))
    }
}

pub async fn send_response(http: &Arc<Http>, message: &Message, error: &str) -> Result<Message, Error> {
    message.channel_id.send_message(&http, CreateMessage::new()
        .reference_message(message)
        .content(error)).await.map_err(Error::new)
}

pub async fn run_bot() {
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot.
    let mut client = Client::builder(Token::from_env("BOT_TOKEN").unwrap(), intents)
        .event_handler(Handler { })
        .await
        .expect("Error creating client");

    // Start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
