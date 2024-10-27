use anyhow::Error;
use crate::database::BeatMap;
use crate::search::SearchError::QueryError;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use surrealdb::engine::remote::ws::Client;
use surrealdb::Surreal;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Argument error")]
    QueryError(),
    #[error("Database error")]
    DatabaseError(#[from] Error),
}

impl SearchError {
    pub fn get_code(&self) -> StatusCode {
        match self {
            QueryError() => StatusCode::BAD_REQUEST,
            SearchError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchArguments {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub results: Vec<BeatMap>,
}

/*
analyzer:
DEFINE ANALYZER ascii TOKENIZERS blank FILTERS ascii, lowercase;
index:
DEFINE INDEX song_name ON TABLE beatmaps FIELDS song SEARCH ANALYZER ascii;
DEFINE INDEX artist_name ON TABLE beatmaps FIELDS artist SEARCH ANALYZER ascii;
DEFINE INDEX charter_name ON TABLE beatmaps FIELDS charter SEARCH ANALYZER ascii;
 */
pub async fn search_database(query: &str, db: Surreal<Client>) -> Result<SearchResult, SearchError> {
    let Ok(arguments) = serde_urlencoded::from_str::<SearchArguments>(query) else {
        return Err(QueryError());
    };

    let mut maps: Vec<BeatMap> = db.query("SELECT * FROM beatmaps WHERE song @@ $query OR artist @@ $query OR charter @@ $query")
        .bind(("query", arguments.query.clone())).await
        .map_err(|err| SearchError::DatabaseError(err.into()))?
        .take(0).map_err(|err| SearchError::DatabaseError(err.into()))?;
    maps.sort_by(|first, second| first.upvotes.cmp(&second.upvotes).reverse());
    Ok(SearchResult {
        query: arguments.query,
        results: maps
    })
}