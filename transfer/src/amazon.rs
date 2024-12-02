use anyhow::Error;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::operation::put_object::PutObjectOutput;
use serde::Serialize;

pub const BUCKET_NAME: &'static str = "beatmap-browser";
pub const MAPS_TABLE_NAME: &'static str = "beatmapbrowser-maps";
pub const USERS_TABLE_NAME: &'static str = "beatmapbrowser-users";
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

    pub async fn upload<T: Serialize>(&self, table: &str, data: &T) -> Result<(), Error> {
        // Upload the item to DynamoDB
        self.db_client
            .put_item()
            .table_name(table)
            .set_item(Some(serde_dynamo::to_item(data)?))
            .send()
            .await?;
        Ok(())
    }
}