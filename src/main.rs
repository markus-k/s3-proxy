use std::ops::Bound;

use axum::{
    body::StreamBody,
    extract::{Extension, Path},
    headers::{HeaderName, Range},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router, TypedHeader,
};
use config::{Configuration, Endpoints};
use s3::{command::Command, request::Reqwest, request_trait::Request, Bucket};

mod config;

#[tracing::instrument]
fn get_bucket_path(request_path: &str, endpoints: &Endpoints) -> Option<String> {
    let endpoint = endpoints
        .iter()
        .find(|endpoint| request_path.starts_with(endpoint.path()));

    tracing::trace!("Found endpoint for request path: {:?}", endpoint);

    if let Some(sub_path) = request_path.strip_prefix(endpoint?.path()) {
        Some(format!(
            "{}/{}",
            endpoint?.bucket_path().trim_end_matches('/'),
            sub_path.trim_start_matches('/')
        ))
    } else {
        None
    }
}

fn copy_headers(destination: &mut HeaderMap, source: &HeaderMap, headers: &[HeaderName]) {
    for header in headers {
        source
            .get(header)
            .and_then(|value| destination.insert(header, value.to_owned()));
    }
}

fn s3_range_for_header(range: Range) -> Option<(u64, Option<u64>)> {
    if range.iter().count() > 1 {
        // AWS S3 only supports one range per request
        None
    } else {
        if let Some((start, end)) = range.iter().next() {
            Some((
                match start {
                    Bound::Unbounded => 0,
                    Bound::Included(start) => start,
                    _ => unreachable!(),
                },
                match end {
                    Bound::Unbounded => None,
                    Bound::Included(end) => Some(end),
                    _ => unreachable!(),
                },
            ))
        } else {
            None
        }
    }
}

fn make_not_found_response() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "File not found")
}

async fn make_proxy_response(
    bucket: &Bucket,
    bucket_path: &str,
    command: Command<'_>,
) -> Result<impl IntoResponse, s3::error::S3Error> {
    let request = Reqwest::new(&bucket, &bucket_path, command);

    let response = request.response().await?;

    let mut headers = HeaderMap::new();
    response
        .content_length()
        .and_then(|len| headers.insert(header::CONTENT_LENGTH, len.into()));

    copy_headers(
        &mut headers,
        response.headers(),
        &[header::CONTENT_TYPE, header::CONTENT_RANGE, header::ETAG],
    );

    let status_code = response.status();
    let body = StreamBody::new(response.bytes_stream());

    Ok((status_code, headers, body).into_response())
}

async fn proxy_request(
    bucket: &Bucket,
    config: &Configuration,
    path: &str,
    command: Command<'_>,
) -> impl IntoResponse {
    let bucket_path = get_bucket_path(path, &config.endpoints());

    if let Some(bucket_path) = bucket_path {
        make_proxy_response(bucket, &bucket_path, command)
            .await
            .map(|r| r.into_response())
            .unwrap_or_else(|err| match err {
                s3::error::S3Error::Http(404, _response) => {
                    make_not_found_response().into_response()
                }
                _ => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("Upstream error: {err}"),
                )
                    .into_response(),
            })
            .into_response()
    } else {
        make_not_found_response().into_response()
    }
}
#[tracing::instrument(skip(bucket))]
async fn get_file(
    Path(path): Path<String>,
    range: Option<TypedHeader<Range>>,
    Extension(bucket): Extension<Bucket>,
    Extension(config): Extension<Configuration>,
) -> impl IntoResponse {
    tracing::info!("GET {}", path);

    let command = if let Some(TypedHeader(range)) = range {
        if let Some((start, end)) = s3_range_for_header(range) {
            Command::GetObjectRange { start, end }
        } else {
            Command::GetObject
        }
    } else {
        Command::GetObject
    };

    proxy_request(&bucket, &config, path.as_str(), command).await
}

#[tracing::instrument(skip(bucket))]
async fn head_file(
    Path(path): Path<String>,
    Extension(bucket): Extension<Bucket>,
    Extension(config): Extension<Configuration>,
) -> impl IntoResponse {
    tracing::info!("HEAD {}", path);

    let command = Command::HeadObject;

    proxy_request(&bucket, &config, path.as_str(), command).await
}

async fn start_server(config: &Configuration) -> anyhow::Result<()> {
    let bucket = config.bucket().make_s3_bucket()?;

    let router = Router::new()
        .route("/*path", get(get_file).head(head_file))
        .layer(Extension(bucket))
        .layer(Extension(config.clone()));

    let bind = config.http().make_socketaddr()?;

    tracing::info!("Listening on http://{bind}/");

    axum::Server::bind(&bind)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

async fn load_configuration() -> anyhow::Result<Configuration> {
    let config_file =
        std::env::var("S3PROXY_CONFIG").unwrap_or_else(|_| "s3-proxy.yaml".to_owned());

    Configuration::from_file(config_file).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    let config = load_configuration().await?;

    start_server(&config).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::Endpoint;

    use super::*;

    #[test]
    fn test_get_bucket_path() {
        let endpoints = Endpoints::from_vec(vec![Endpoint::new(
            "/media/".to_owned(),
            "/app/files".to_owned(),
        )]);

        let bucket_path = get_bucket_path("/media/foo/bar", &endpoints);

        assert_eq!(bucket_path.as_deref(), Some("/app/files/foo/bar"));
    }
}
