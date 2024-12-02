use std::collections::HashMap;
use anyhow::Error;
use aws_config::meta::region::RegionProviderChain;
use aws_config::Region;
use aws_sdk_dynamodb::operation::update_item::builders::UpdateItemFluentBuilder;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3::config::BehaviorVersion;
use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
use aws_sdk_s3::operation::put_object::PutObjectOutput;
use serde::{Deserialize, Serialize};
use serde_dynamo::Item;
use crate::api::APIError;
use crate::util::database::{AccountLink, BeatMap};
use crate::util::get_search_combos;

pub const BUCKET_NAME: &'static str = "beatmap-browser";
pub const MAPS_TABLE_NAME: &'static str = "beatmapbrowser-maps";
pub const USERS_TABLE_NAME: &'static str = "beatmapbrowser-users";
pub const TOKENS_TABLE_NAME: &'static str = "beatmapbrowser-tokens";
pub const BUCKET_REGION: &'static str = "us-east-2";

#[derive(Clone)]
pub struct Amazon {
    s3_client: aws_sdk_s3::Client,
    db_client: aws_sdk_dynamodb::Client,
}

pub async fn setup() -> Result<Amazon, Error> {
    let region_provider = RegionProviderChain::first_try(Region::new(BUCKET_REGION));
    let shared_config = aws_config::defaults(BehaviorVersion::latest()).region(region_provider).load().await;
    Ok(Amazon {
        s3_client: aws_sdk_s3::Client::new(&shared_config),
        db_client: aws_sdk_dynamodb::Client::new(&shared_config),
    })
}

impl Amazon {
    pub async fn upload_object(
        &self,
        file: Vec<u8>,
        file_name: &str,
    ) -> Result<PutObjectOutput, Error> {
        self.s3_client
            .put_object()
            .bucket(BUCKET_NAME)
            .key(file_name)
            .body(file.into())
            .send()
            .await
            .map_err(Error::from)
    }

    pub async fn update<F: Fn(UpdateItemFluentBuilder) -> UpdateItemFluentBuilder>(&self,
                        table_name: &'static str, id: String, updater: F) -> Result<(), Error> {
        updater(self.db_client
            .update_item()
            .table_name(table_name)
            .key("id", AttributeValue::S(id)))
            .send()
            .await?;
        Ok(())
    }

    pub async fn delete_object(
        &self,
        file_name: &str,
    ) -> Result<DeleteObjectOutput, Error> {
        self.s3_client
            .delete_object()
            .bucket(BUCKET_NAME)
            .key(file_name)
            .send()
            .await
            .map_err(Error::from)
    }

    pub async fn upload_song(&self, song: &BeatMap) -> Result<(), Error> {
        let prefixes = get_search_combos(&song);
        self.upload(MAPS_TABLE_NAME, &song, Some(&HashMap::from([("title_prefix".to_string(), prefixes)]))).await
    }

    pub async fn upload<T: Serialize, A: Serialize>(&self, table: &str, data: &T, adding: Option<&A>) -> Result<(), Error> {
        // Convert the struct to a DynamoDB item
        let mut item: HashMap<String, AttributeValue> = serde_dynamo::to_item(data)?;
        if let Some(adding) = adding {
            for (k, v) in serde_dynamo::to_item::<&A, Item>(adding)?.into_inner() {
                item.insert(k, v.into());
            }
        }

        // Upload the item to DynamoDB
        self.db_client
            .put_item()
            .table_name(table)
            .set_item(Some(item))
            .send()
            .await?;
        Ok(())
    }

    pub async fn search_songs(
        &self,
        query: &str,
    ) -> Result<Vec<BeatMap>, Error> {
        let trying = query.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty())
            .take(3);
        let mut found: HashMap<String, (BeatMap, u64)> = HashMap::new();
        // Perform a query on the GSI
        for possible in trying.clone() {
            for result in self.db_client
                .scan()
                .table_name(MAPS_TABLE_NAME)
                .filter_expression("contains(#title_prefix, :target_string)")
                .expression_attribute_names("#title_prefix", "title_prefix")
                .expression_attribute_values(":target_string", AttributeValue::S(possible.to_lowercase()))
                .send()
                .await?
                .items
                .unwrap_or_default() {
                found.entry(result["id"].as_s().map_err(|_| Error::msg("Unexpected ID type"))?.clone())
                    .or_insert((serde_dynamo::from_item(result)?, 0))
                    .1 += 1;
            }
        }
        let mut values: Vec<_> = found.into_values().collect();
        values.sort_by_key(|(map, count)| (*count * 1000000000) + map.upvotes);
        Ok(values.into_iter().map(|(map, _)| map).collect())
    }

    pub async fn query_by_link<T: for<'a> Deserialize<'a>>(
        &self,
        link: AccountLink,
    ) -> Result<Option<T>, Error> {
        let found = self.db_client
               .scan()
               .table_name(USERS_TABLE_NAME)
               .filter_expression("contains(links, :google_obj)")
               .expression_attribute_values(
                   ":google_obj",
                   AttributeValue::M(
                       HashMap::from([
                           ("google".to_string(), AttributeValue::S(link.id()))
                       ]),
                   ),
               )
               .send()
               .await?;
        Ok(found.items.ok_or(Error::msg("No items found"))?
            .into_iter()
            .next()
            .map(|item| serde_dynamo::from_item::<_, T>(item))
            .transpose()?)
    }

    pub async fn query_one<T: for<'a> Deserialize<'a>>(
        &self,
        table: &'static str,
        field: &str,
        value: String
    ) -> Result<Option<T>, Error> {
        Ok(self.query(table, field, value).await?.into_iter().next())
    }

    pub async fn query<T: for<'a> Deserialize<'a>>(
        &self,
        table: &'static str,
        field: &str,
        value: String
    ) -> Result<Vec<T>, Error> {
        // Perform a query on the GSI
        let result = self.db_client
            .query()
            .table_name(table)
            .index_name(format!("{}-index", field))
            .key_condition_expression(format!("{} = :input", field))
            .expression_attribute_values(":input", AttributeValue::S(value.to_string()))
            .send()
            .await?;
        Ok(result.items
            .unwrap_or_default()
            .into_iter()
            .map(|item| serde_dynamo::from_item(item))
            .collect::<Result<Vec<T>, serde_dynamo::Error>>()?)
    }

    pub async fn remove(
        &self,
        table: &'static str,
        field: &str,
        value: String
    ) -> Result<(), Error> {
        self.db_client
            .delete_item()
            .table_name(table)
            .key(field, AttributeValue::S(value))
            .send()
            .await?;
        Ok(())
    }

    pub async fn add_to_list(&self, table: &'static str, id: String, field: &str, adding: String) -> Result<(), APIError> {
        self.update(table, id, |builder| {
                builder
                    .update_expression(format!("SET {} = list_append(if_not_exists(upvoted, :empty_list), :new_value)", field))
                    .expression_attribute_values(":empty_list", AttributeValue::L(vec![])) // Default to an empty list if `upvoted` does not exist
                    .expression_attribute_values(":new_value", AttributeValue::L(vec![AttributeValue::S(adding.to_string())]))
            })
            .await.map_err(APIError::database_error)
    }

    pub async fn overwrite_list<T: ToString>(&self, table: &'static str, id: String, field: &str, new_list: Vec<T>) -> Result<(), APIError> {
        self.update(table, id, |builder| {
            builder
                .update_expression(format!("SET {} = :updated_list", field))
                .expression_attribute_values(":updated_list", AttributeValue::L(new_list.iter()
                    .map(|elem| AttributeValue::S(elem.to_string())).collect()))
        })
            .await.map_err(APIError::database_error)
    }
}