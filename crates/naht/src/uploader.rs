//! The real Open Cloud asset uploader (Stage 12).
//!
//! `naht-core` defines the [`AssetUploader`] interface and the upload-once/cache policy with no
//! network I/O; this is the binary-side client that actually talks to Roblox Open Cloud.
//!
//! The API key never appears in config or code, only the *name* of an environment variable holding
//! it. The blocking HTTP runs on a dedicated thread so it doesn't panic inside the async runtime.
//!
//! Note: the upload is exercised by `naht-core`'s tests through a fake; this real path needs a live
//! key to validate, and the Open Cloud asset operation is asynchronous in reality (the response may
//! be an operation to poll). The id extraction here is best-effort.

use std::env::{self, VarError};

use naht_core::assets::{AssetError, AssetUploader, UploadFailure};
use thiserror::Error;

use crate::config::AssetsConfig;
use reqwest::blocking::Client;
use reqwest::blocking::multipart::Form;
use reqwest::blocking::multipart::Part;
use reqwest::redirect;
use std::fmt;
use std::thread;
use std::time::Duration;

const ASSETS_ENDPOINT: &str = "https://apis.roblox.com/assets/v1/assets";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Why the Open Cloud client could not be built from the `[assets]` config.
#[derive(Debug, Error)]
pub enum UploaderError {
    /// Uploads are enabled but no environment variable name was configured.
    #[error("[assets] is enabled but `api_key_env` is not set")]
    KeyNameMissing,
    /// The configured environment variable is unset or not valid Unicode.
    #[error("cannot read the Open Cloud API key from `${name}`")]
    KeyUnreadable {
        /// The configured environment variable name, never its value.
        name: String,
        /// Why the variable could not be read.
        #[source]
        source: VarError,
    },
}

/// An Open Cloud client holding the resolved API key.
pub struct OpenCloudUploader {
    api_key: String,
}

impl fmt::Debug for OpenCloudUploader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenCloudUploader").finish_non_exhaustive()
    }
}

impl OpenCloudUploader {
    /// Build the uploader from config, reading the API key from the named environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`UploaderError`] when the API key variable is missing or the client cannot be built.
    pub fn from_config(assets: &AssetsConfig) -> Result<Self, UploaderError> {
        let env_name = assets
            .api_key_env
            .as_deref()
            .ok_or(UploaderError::KeyNameMissing)?;
        let api_key = env::var(env_name).map_err(|source| UploaderError::KeyUnreadable {
            name: env_name.to_owned(),
            source,
        })?;
        Ok(Self { api_key })
    }
}

impl AssetUploader for OpenCloudUploader {
    fn upload(&self, name: &str, content: &[u8]) -> Result<String, AssetError> {
        isolated_on_scoped_thread(|| upload_blocking(&self.api_key, name, content))
    }
}

fn isolated_on_scoped_thread<T: Send>(
    work: impl FnOnce() -> Result<T, AssetError> + Send,
) -> Result<T, AssetError> {
    thread::scope(|scope| match scope.spawn(work).join() {
        Ok(result) => result,
        Err(_) => Err(AssetError::Upload(UploadFailure::WorkerPanicked)),
    })
}

fn upload_blocking(api_key: &str, name: &str, content: &[u8]) -> Result<String, AssetError> {
    let client = build_client(CONNECT_TIMEOUT, REQUEST_TIMEOUT).map_err(transport)?;
    upload_with(&client, ASSETS_ENDPOINT, api_key, name, content)
}

fn build_client(connect: Duration, total: Duration) -> Result<Client, reqwest::Error> {
    Client::builder()
        .redirect(redirect::Policy::none())
        .connect_timeout(connect)
        .timeout(total)
        .build()
}

fn transport(error: reqwest::Error) -> AssetError {
    AssetError::Upload(UploadFailure::Transport {
        source: Box::new(error),
    })
}

fn upload_with(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    name: &str,
    content: &[u8],
) -> Result<String, AssetError> {
    let request = serde_json::json!({
        "assetType": asset_type(name),
        "displayName": name,
        "description": "Uploaded by Naht",
    });
    let form = Form::new().text("request", request.to_string()).part(
        "fileContent",
        Part::bytes(content.to_vec()).file_name(name.to_owned()),
    );

    let response = client
        .post(endpoint)
        .header("x-api-key", api_key)
        .multipart(form)
        .send()
        .map_err(transport)?;
    if !response.status().is_success() {
        return Err(AssetError::Upload(UploadFailure::Rejected(
            response.status().as_u16(),
        )));
    }
    let body: serde_json::Value = response.json().map_err(|error| {
        AssetError::Upload(UploadFailure::Unreadable {
            source: Box::new(error),
        })
    })?;
    extract_asset_id(&body).ok_or(AssetError::Upload(UploadFailure::MissingAssetId))
}

/// A coarse asset-type guess from the file extension; refined as more types are supported.
fn asset_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "bmp" | "tga") => "Image",
        Some("mp3" | "ogg" | "wav") => "Audio",
        _ => "Model",
    }
}

fn extract_asset_id(body: &serde_json::Value) -> Option<String> {
    let id = body
        .get("response")
        .and_then(|response| response.get("assetId"))
        .or_else(|| body.get("assetId"))?;
    let id = id
        .as_str()
        .map(str::to_owned)
        .or_else(|| id.as_u64().map(|number| number.to_string()))?;
    Some(format!("rbxassetid://{id}"))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use naht_core::assets::{AssetError, UploadFailure};

    use super::{build_client, upload_with};

    fn read_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut received = Vec::new();
        let mut buffer = [0_u8; 4096];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8_lossy(&received).into_owned()
    }

    fn redirecting_endpoint() -> (String, mpsc::Receiver<String>) {
        let (seen_tx, seen_rx) = mpsc::channel();
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        let target_url = format!("http://{}/assets", target.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = target.accept().unwrap();
            seen_tx.send(read_request(&mut stream)).unwrap();
        });

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/upload", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (url, seen_rx)
    }

    #[test]
    fn redirects_are_not_followed_so_the_api_key_stays_on_the_endpoint() {
        let (url, seen) = redirecting_endpoint();
        let client = build_client(Duration::from_secs(5), Duration::from_secs(5)).unwrap();

        let result = upload_with(&client, &url, "secret-key", "a.png", b"pixels");

        assert!(matches!(
            result,
            Err(AssetError::Upload(UploadFailure::Rejected(302)))
        ));
        assert!(
            seen.recv_timeout(Duration::from_millis(500)).is_err(),
            "the redirect target received a request"
        );
    }

    #[test]
    fn a_stalled_endpoint_fails_within_the_request_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/upload", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(5));
        });
        let client = build_client(Duration::from_secs(5), Duration::from_millis(300)).unwrap();

        let started = Instant::now();
        let result = upload_with(&client, &url, "secret-key", "a.png", b"pixels");

        assert!(matches!(
            result,
            Err(AssetError::Upload(UploadFailure::Transport { .. }))
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
