# `s3-proxy`, a reverse proxy for S3 buckets
[![Rust](https://github.com/markus-k/s3-proxy/actions/workflows/rust.yml/badge.svg)](https://github.com/markus-k/s3-proxy/actions/workflows/rust.yml)
![License](https://img.shields.io/github/license/markus-k/s3-proxy)

`s3-proxy` is a "cloud-native" reverse proxy for S3 buckets.

It is designed for small to mid-sized applications that don't need a full blown
CDN in front of their S3 bucket, but still want to keep state out of their 
containers, e.g. to not have a local file system for uploads and media files.

Features:
* [x] Multiple endpoint defintions
* [ ] Gzip-Compression
* [ ] Caching (TBD, introduces read-inconsistency)
* [ ] Access control with temparary tokens for protected files

## Project status

`s3-proxy` is still in very early development and therefore not ready for 
production use.

## Usage

`s3-proxy` can be run on bare-metal, or as a container. The latter is usually
preferred, as the typical use case is already a Docker or Kubernetes scenario.

TODO: provide docker images and describe their usage .

## Configuration

`s3-proxy` is configured using a `s3-proxy.yaml`-File. A typical use case might
use a configuration similar to this:

``` yaml
bucket:
  endpoint: "https://s3.fr-par.scw.cloud"
  region: "fr-par"
  bucket_name: "my-apps-files"
  # access and secret key can be configured here or via the environment 
  # variables AWS_S3_ACCESS_KEY_ID and AWS_S3_SECRET_KEY
  access_key: ABCDEF
  secret-key: 0987654321-1234567890

endpoints:
    # all requests to files unter /media/* are proxied to the S3 path
    # /my-app/media/*. Endpoints are sorted by length and then handled on 
    # a first-match basis.
  - path: "/media/"
    bucket_path: "/my-app/media/"

  - path: "/pdfs/"
    bucket_path: "/pdfs/"

http:
  bind: "0.0.0.0"
  port: 8000
```

## License

`s3-proxy` is licensed under the Apache 2.0-License.
