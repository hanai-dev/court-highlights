//! Object storage (Cloudflare R2 in production, any S3-compatible store in tests). Boxes upload and players
//! download straight from the store with short-lived presigned URLs, so video never passes through this service.

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::{Client, config::Credentials};
use std::time::Duration;

#[derive(Clone)]
pub struct Store {
    bucket: String,
    /// Signs URLs for the boxes (and does housekeeping).
    internal: Client,
    /// Signs URLs for players' phones; differs from `internal` only when the store is reached by another name.
    public: Client,
}

fn client(endpoint: &str, region: &str, key: &str, secret: &str) -> Client {
    let http = aws_smithy_http_client::Builder::new()
        .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring))
        .build_https();
    let conf = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .http_client(http)
        .region(aws_sdk_s3::config::Region::new(region.to_string()))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .credentials_provider(Credentials::new(key, secret, None, None, "ch-cloud"))
        .build();
    Client::from_conf(conf)
}

pub type StoreError = Box<dyn std::error::Error + Send + Sync>;

impl Store {
    pub fn new(endpoint: &str, public_endpoint: &str, region: &str, bucket: &str, key: &str, secret: &str) -> Self {
        Self { bucket: bucket.into(), internal: client(endpoint, region, key, secret), public: client(public_endpoint, region, key, secret) }
    }

    /// Creates the bucket when it does not exist (development and test stores; R2 buckets are made by hand).
    pub async fn ensure_bucket(&self) -> Result<(), StoreError> {
        if self.internal.head_bucket().bucket(&self.bucket).send().await.is_err() {
            self.internal.create_bucket().bucket(&self.bucket).send().await?;
        }
        Ok(())
    }

    pub async fn upload_url(&self, key: &str) -> Result<String, StoreError> {
        let req = self.internal.put_object().bucket(&self.bucket).key(key).content_type("video/mp4");
        Ok(req.presigned(PresigningConfig::expires_in(Duration::from_secs(15 * 60))?).await?.uri().to_string())
    }

    pub async fn download_url(&self, key: &str, filename: &str) -> Result<String, StoreError> {
        let req = self
            .public
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .response_content_disposition(format!("attachment; filename=\"{filename}\""))
            .response_content_type("video/mp4");
        Ok(req.presigned(PresigningConfig::expires_in(Duration::from_secs(60 * 60))?).await?.uri().to_string())
    }

    /// Size of a stored object, or None when it is not there.
    pub async fn size(&self, key: &str) -> Option<i64> {
        self.internal.head_object().bucket(&self.bucket).key(key).send().await.ok().and_then(|h| h.content_length())
    }

    pub async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.internal.delete_object().bucket(&self.bucket).key(key).send().await?;
        Ok(())
    }
}
