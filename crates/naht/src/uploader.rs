//! The real Open Cloud asset uploader (Stage 12). `naht-core` defines the [`AssetUploader`] interface
//! and the upload-once/cache policy with no network I/O; this is the binary-side client that actually
//! talks to Roblox Open Cloud.
//!
//! The API key never appears in config or code — only the *name* of an environment variable holding
//! it. The blocking HTTP runs on a dedicated thread so it doesn't panic inside the async runtime.
//!
//! Note: the upload is exercised by `naht-core`'s tests through a fake; this real path needs a live
//! key to validate, and the Open Cloud asset operation is asynchronous in reality (the response may
//! be an operation to poll). The id extraction here is best-effort.

use anyhow::{Context, Result};
use naht_core::assets::{AssetError, AssetUploader};

use crate::config::AssetsConfig;

const ASSETS_ENDPOINT: &str = "https://apis.roblox.com/assets/v1/assets";

/// An Open Cloud client holding the resolved API key.
pub struct OpenCloudUploader {
    api_key: String,
}

impl OpenCloudUploader {
    /// Build the uploader from config, reading the API key from the named environment variable.
    pub fn from_config(assets: &AssetsConfig) -> Result<Self> {
        let env_name = assets
            .api_key_env
            .as_deref()
            .context("[assets] is enabled but `api_key_env` is not set")?;
        let api_key = std::env::var(env_name)
            .with_context(|| format!("reading the Open Cloud API key from ${env_name}"))?;
        Ok(Self { api_key })
    }
}

impl AssetUploader for OpenCloudUploader {
    fn upload(&self, name: &str, content: &[u8]) -> Result<String, AssetError> {
        // Run the blocking request off the async runtime: reqwest::blocking panics if called from a
        // runtime thread, so isolate it on a scoped thread.
        std::thread::scope(|scope| {
            match scope
                .spawn(|| upload_blocking(&self.api_key, name, content))
                .join()
            {
                Ok(result) => result,
                Err(_) => Err(AssetError::Upload("upload thread panicked".to_string())),
            }
        })
    }
}

fn upload_blocking(api_key: &str, name: &str, content: &[u8]) -> Result<String, AssetError> {
    let request = serde_json::json!({
        "assetType": asset_type(name),
        "displayName": name,
        "description": "Uploaded by Naht",
    });
    let form = reqwest::blocking::multipart::Form::new()
        .text("request", request.to_string())
        .part(
            "fileContent",
            reqwest::blocking::multipart::Part::bytes(content.to_vec()).file_name(name.to_string()),
        );

    let response = reqwest::blocking::Client::new()
        .post(ASSETS_ENDPOINT)
        .header("x-api-key", api_key)
        .multipart(form)
        .send()
        .map_err(|error| AssetError::Upload(error.to_string()))?;
    if !response.status().is_success() {
        return Err(AssetError::Upload(format!(
            "Open Cloud returned {}",
            response.status()
        )));
    }
    let body: serde_json::Value = response
        .json()
        .map_err(|error| AssetError::Upload(error.to_string()))?;
    extract_asset_id(&body)
        .ok_or_else(|| AssetError::Upload("no asset id in the Open Cloud response".to_string()))
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
        .map(str::to_string)
        .or_else(|| id.as_u64().map(|n| n.to_string()))?;
    Some(format!("rbxassetid://{id}"))
}
