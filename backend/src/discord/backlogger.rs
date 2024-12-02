use crate::discord::{Handler, WHITELISTED_CHANNELS, WHITELISTED_GUILDS};
use anyhow::Error;
use futures::Stream;
use serenity::all::{ChannelId, Context, GuildChannel, GuildId, Http, Message, Timestamp};
use serenity::futures::{stream, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;

pub async fn update_backlog(handler: &Handler, context: &Context) -> Result<(), Error> {
    for guild in WHITELISTED_GUILDS {
        for thread in GuildId::new(guild)
            .get_active_threads(&context.http)
            .await?
            .threads
        {
            if !WHITELISTED_CHANNELS.contains(&thread.parent_id.unwrap_or(ChannelId::new(1)).into())
            {
                continue;
            }

            if let Err(error) = update_channel(thread.id, handler.clone(), context.http.clone()).await {
                println!("Error updating backlog: {error:?}")
            }
        }
    }
    for channel in WHITELISTED_CHANNELS {
        for thread in ThreadIter::<&Arc<Http>>::stream(&context.http, ChannelId::new(channel))
            .take_while(|result| {
                let output = result.is_ok();
                async move { output }
            })
            .collect::<Vec<_>>()
            .await
        {
            let thread = match thread {
                Ok(thread) => thread,
                Err(error) => {
                    println!("Error updating backlog for thread: {error:?}");
                    continue
                }
            };


            if let Err(error) = update_channel(thread.id, handler.clone(), context.http.clone()).await {
                println!("Error updating backlog: {error:?}")
            }
        }
        if let Err(error) = update_channel(
            ChannelId::new(channel),
            handler.clone(),
            context.http.clone(),
        )
        .await {
            println!("Error updating backlog: {error:?}")
        }
    }
    println!("Done updating backlog!");
    Ok(())
}

async fn update_channel(
    channel: ChannelId,
    handler: Handler,
    http: Arc<Http>,
) -> Result<(), Error> {
    let output: Vec<Message> = channel
        .messages_iter(&http)
        .take_while(|message| {
            let value = message
                .as_ref()
                .map(|inner| !inner.reactions.iter().any(|reaction| reaction.me))
                .map_err(|err| {
                    println!("HTTP Error: {}", err);
                    err
                })
                .unwrap_or(true);
            async move { value }
        })
        .filter_map(|value| async move { value.ok() })
        .collect::<Vec<_>>()
        .await;
    let upvotes = output
        .iter()
        .map(|msg| msg.author.id)
        .collect::<HashSet<_>>();
    if !stream::iter(output)
        .any(|message| handler.handle_message_dropped(&http, message, &upvotes))
        .await
    {
        //println!("Failed for thread {}", channel);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ThreadIter<H: AsRef<Http>> {
    http: H,
    channel_id: ChannelId,
    buffer: Vec<GuildChannel>,
    before: Option<Timestamp>,
    tried_fetch: bool,
}

impl<H: AsRef<Http>> ThreadIter<H> {
    fn new(http: H, channel_id: ChannelId) -> Self {
        Self {
            http,
            channel_id,
            buffer: Vec::new(),
            before: None,
            tried_fetch: false,
        }
    }

    async fn refresh(&mut self) -> serenity::Result<()> {
        let grab_size = 100;

        self.buffer = self
            .channel_id
            .get_archived_public_threads(self.http.as_ref(), self.before, Some(grab_size))
            .await?
            .threads
            .into_iter()
            .collect();

        self.buffer.reverse();

        self.before = self
            .buffer
            .first()
            .map(|m| m.thread_metadata.unwrap().archive_timestamp.unwrap());

        self.tried_fetch = true;

        Ok(())
    }

    pub fn stream(
        http: impl AsRef<Http>,
        channel_id: ChannelId,
    ) -> impl Stream<Item = serenity::Result<GuildChannel>> {
        let init_state = ThreadIter::new(http, channel_id);

        futures::stream::unfold(init_state, |mut state| async {
            if state.buffer.is_empty() && state.before.is_some() || !state.tried_fetch {
                if let Err(error) = state.refresh().await {
                    println!("Err: {error:?}");
                    return Some((Err(error), state));
                }
            }

            // the resultant stream goes from newest to oldest.
            state.buffer.pop().map(|entry| (Ok(entry), state))
        })
    }
}
