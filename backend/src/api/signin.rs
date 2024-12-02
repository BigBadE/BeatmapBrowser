use crate::api::APIError;
use crate::util::amazon::{MAPS_TABLE_NAME, TOKENS_TABLE_NAME, USERS_TABLE_NAME};
use crate::util::database::{AccountLink, User, UserID};
use crate::util::warp::Replyable;
use crate::util::{data, get_user_from_link};
use anyhow::Error;
use aws_sdk_dynamodb::types::AttributeValue;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use firebase_auth::FirebaseUser;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::header::HeaderMap;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::env;
use warp::{Rejection, Reply};
use crate::api::upvote::unvote_for_map;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscordTokenRequest {
    pub access_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub global_name: String,
    pub verified: bool,
}

pub async fn discord_signin(code: String) -> Result<impl Reply, Rejection> {
    let account = get_user_from_link(AccountLink::Discord(
        get_discord_user(code).await?.id.parse().unwrap(),
    ))
    .await?;
    Ok(account.id.to_string().reply())
}

pub async fn discord_sync(user: User, code: String) -> Result<impl Reply, Rejection> {
    let other = get_user_from_link(AccountLink::Discord(
        get_discord_user(code).await?.id.parse().unwrap(),
    ))
    .await?;
    merge(user, other).await?;
    Ok("Ok".reply())
}

pub async fn google_signin(code: String) -> Result<impl Reply, Rejection> {
    let user: FirebaseUser = data()
        .await
        .auth
        .verify(&code)
        .map_err(|err| APIError::AuthError(err.to_string()))?;
    let account = get_user_from_link(AccountLink::Google(user.user_id)).await?;
    Ok(get_token(account.id)
        .await
        .map_err(APIError::database_error)?
        .reply())
}

pub async fn google_sync(user: User, code: String) -> Result<impl Reply, Rejection> {
    let firebase_user: FirebaseUser = data()
        .await
        .auth
        .verify(&code)
        .map_err(|err| APIError::AuthError(err.to_string()))?;
    let other = get_user_from_link(AccountLink::Google(firebase_user.user_id)).await?;
    merge(user, other).await?;
    Ok("Ok".reply())
}

pub async fn merge(mut first: User, second: User) -> Result<(), APIError> {
    for map in second.maps {
        data()
            .await
            .amazon
            .update(MAPS_TABLE_NAME, map.to_string(), |update| {
                update
                    .update_expression("SET charter_uid = :charter")
                    .expression_attribute_values(
                        ":charter",
                        AttributeValue::S(first.id.to_string()),
                    )
            })
            .await
            .map_err(APIError::database_error)?;
    }
    first.downloaded.extend(second.downloaded);
    for unvoting in first.upvoted.clone().iter().filter(|map| second.upvoted.contains(map)) {
        let unvoting = data().await.amazon.query_one(MAPS_TABLE_NAME, "id", unvoting.to_string()).await.map_err(APIError::database_error)?
            .ok_or(APIError::DatabaseError(Error::msg("Failed to find map while merging!")))?;
        unvote_for_map(&unvoting, &mut first).await?;
    }
    first.upvoted.extend(second.upvoted);
    data()
        .await
        .amazon
        .upload(USERS_TABLE_NAME, &first, None::<&Vec<String>>)
        .await
        .map_err(APIError::database_error)?;
    data()
        .await
        .amazon
        .remove(USERS_TABLE_NAME, "id", second.id.to_string())
        .await
        .map_err(APIError::database_error)?;
    Ok(())
}

pub async fn get_discord_user(code: String) -> Result<DiscordUser, APIError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    let secret = env::var("CLIENT_SECRET").unwrap();
    let params = [
        ("client_id", "1298420686087262269"),
        ("client_secret", secret.as_str()),
        ("grant_type", "authorization_code"),
        ("code", &*code),
        (
            "redirect_uri",
            "https://beatblockbrowser.me/api/discordauth",
        ),
        ("scope", "identify"),
    ];
    let token = Client::builder()
        .default_headers(headers.clone())
        .build()
        .map_err(APIError::database_error)?
        .post("https://discord.com/api/v10/oauth2/token")
        .form(&params)
        .send()
        .await
        .map_err(APIError::database_error)?;
    let token: DiscordTokenRequest = get_response(token).await?;

    headers.insert(
        "Authorization",
        format!("Bearer {}", token.access_token).parse().unwrap(),
    );
    let user = Client::builder()
        .default_headers(headers)
        .build()
        .map_err(APIError::database_error)?
        .get("https://discord.com/api/users/@me")
        .send()
        .await
        .map_err(APIError::database_error)?;
    let user: DiscordUser = get_response(user).await?;
    if !user.verified {
        return Err(APIError::AuthError("Your account's email isn't verified.".to_string()).into());
    }
    Ok(user)
}

pub async fn get_token(user: UserID) -> Result<String, Error> {
    if let Some(token) = data()
        .await
        .amazon
        .query_one::<UserToken>(TOKENS_TABLE_NAME, "id", user.to_string())
        .await?
    {
        return Ok(token.token);
    }

    let mut rng = OsRng; // Uses the operating system's randomness
    let mut buffer = [0u8; 32]; // 32 bytes = 256 bits
    rng.fill_bytes(&mut buffer);

    // Encode the random bytes as a Base64 string
    let token = BASE64_STANDARD.encode(&buffer);
    data()
        .await
        .amazon
        .upload(
            TOKENS_TABLE_NAME,
            &UserToken {
                id: user,
                token: token.clone(),
            },
            None::<&Vec<String>>,
        )
        .await?;
    Ok(token)
}

async fn get_response<T: for<'a> Deserialize<'a>>(response: Response) -> Result<T, APIError> {
    let bytes = response.bytes().await.map_err(APIError::database_error)?;
    let string = String::from_utf8_lossy(&bytes).into_owned();
    let deserialized = serde_json::from_str(&string).map_err(|e| {
        println!("{}", string);
        APIError::database_error(e)
    })?;
    Ok(deserialized)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserToken {
    pub id: UserID,
    #[serde(rename = "user_token")]
    token: String,
}
