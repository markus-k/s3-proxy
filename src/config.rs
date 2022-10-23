use std::{net::SocketAddr, path::Path};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Couldn't parse region")]
    UnkownRegion,
    #[error("Couldn't parse bind address: {0}")]
    AddrParseError(#[from] std::net::AddrParseError),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Bucket {
    endpoint: Option<String>,
    region: String,
    bucket_name: String,
    access_key: Option<String>,
    secret_key: Option<String>,
}

impl Bucket {
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn bucket_name(&self) -> &str {
        &self.bucket_name
    }

    /// Returns the configured access key.
    ///
    /// If no key is configured, it tried to get `AWS_S3_ACCESS_KEY_ID` from the
    /// environment. If that environment variable is not set, [`None`] is returned.
    pub fn access_key(&self) -> Option<String> {
        self.access_key
            .as_deref()
            .and_then(|s| Some(s.to_owned()))
            .or_else(|| std::env::var("AWS_S3_ACCESS_KEY_ID").ok())
    }

    /// Returns the configured secret key.
    ///
    /// If no key is configured, it tried to get `AWS_S3_SECRET_KEY` from the
    /// environment. If that environment variable is not set, [`None`] is returned.
    pub fn secret_key(&self) -> Option<String> {
        self.secret_key
            .as_deref()
            .and_then(|s| Some(s.to_owned()))
            .or_else(|| std::env::var("AWS_S3_SECRET_KEY").ok())
    }

    pub fn make_s3_region(&self) -> Result<s3::region::Region, ConfigError> {
        if let Some(endpoint) = self.endpoint() {
            Ok(s3::Region::Custom {
                region: self.region().to_owned(),
                endpoint: endpoint.to_owned(),
            })
        } else {
            self.region.parse().map_err(|_e| ConfigError::UnkownRegion)
        }
    }

    pub fn make_s3_bucket(&self) -> Result<s3::Bucket, ConfigError> {
        let credentials = s3::creds::Credentials::new(
            self.access_key().as_deref(),
            self.secret_key().as_deref(),
            None,
            None,
            None,
        )
        .unwrap();

        let mut bucket = s3::Bucket::new(self.bucket_name(), self.make_s3_region()?, credentials)
            .expect("Bucket::new panicked, that shouldn't happen.");

        bucket.set_path_style(); // this should probably be configurable

        Ok(bucket)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Endpoint {
    path: String,
    bucket_path: String,
}

impl Endpoint {
    pub fn new(path: String, bucket_path: String) -> Self {
        Self { path, bucket_path }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn bucket_path(&self) -> &str {
        &&self.bucket_path
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Endpoints(Vec<Endpoint>);

impl Endpoints {
    pub fn from_vec(vec: Vec<Endpoint>) -> Self {
        let mut endpoints = Self(vec);
        endpoints.sort_endpoints();

        endpoints
    }

    fn sort_endpoints(&mut self) {
        // reverse sort by length, so the longest path comes first
        self.0.sort_by_key(|key| key.path().len());
        self.0.reverse();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Endpoint> {
        self.0.iter()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Http {
    bind: String,
    port: u16,
}

impl Default for Http {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_owned(),
            port: 8000,
        }
    }
}

impl Http {
    pub fn bind(&self) -> &str {
        &self.bind
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Creates a [`SocketAddr`] from the instances [`bind`] and [`port`].
    pub fn make_socketaddr(&self) -> Result<SocketAddr, ConfigError> {
        Ok(format!("{}:{}", self.bind(), self.port()).parse()?)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Configuration {
    bucket: Bucket,
    endpoints: Endpoints,
    http: Http,
}

impl Configuration {
    pub async fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let yaml = tokio::fs::read_to_string(path).await?;

        let mut config = serde_yaml::from_str::<Self>(&yaml)?;
        config.initialize();

        Ok(config)
    }

    fn initialize(&mut self) {
        self.endpoints.sort_endpoints();
    }

    pub fn bucket(&self) -> &Bucket {
        &self.bucket
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub fn http(&self) -> &Http {
        &self.http
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_configuration_with_custom_endpoint() {
        let conf = Bucket {
            endpoint: Some("https://s3.fr-par.scw.cloud".to_owned()),
            region: "fr-par".to_owned(),
            bucket_name: "test".to_owned(),
            access_key: None,
            secret_key: None,
        };

        assert_eq!(conf.endpoint().unwrap(), "https://s3.fr-par.scw.cloud");
        assert_eq!(conf.region(), "fr-par");
        assert_eq!(conf.bucket_name(), "test");

        assert!(matches!(
            conf.make_s3_region().unwrap(),
            s3::region::Region::Custom { region, endpoint }
            if &region == "fr-par" && &endpoint == "https://s3.fr-par.scw.cloud"
        ));
    }

    #[test]
    fn test_bucket_configuration_without_custom_endpoint() {
        let conf = Bucket {
            endpoint: None,
            region: "eu-west-1".to_owned(),
            bucket_name: "test".to_owned(),
            access_key: None,
            secret_key: None,
        };

        assert!(conf.endpoint().is_none());
        assert_eq!(conf.region(), "eu-west-1");
        assert_eq!(conf.bucket_name(), "test");

        assert!(matches!(
            conf.make_s3_region().unwrap(),
            s3::region::Region::EuWest1
        ));
    }
}
