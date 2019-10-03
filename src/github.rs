use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Result;

pub type Releases = Vec<Release>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Release {
    url: String,
    id: u32,
    tag_name: String,
    created_at: DateTime<Utc>,
    published_at: DateTime<Utc>,
    assets: Vec<Asset>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    url: String,
    id: u32,
    name: String,
    content_type: String, // application/octet-stream
    size: u64,
    browser_download_url: String,
}

pub struct Credentials {
    user: String,
    password: String,
}

pub struct GitHub {
    creds: Option<Credentials>,
}

impl GitHub {
    pub fn releases(user: &str, repo: &str) -> Result<Releases> {
        let body = reqwest::get(&format!(
            "https://api.github.com/repos/{}/{}/releases",
            user, repo
        ))
        .unwrap()
        .text()
        .unwrap();

        let releases = serde_json::from_str(&body)?;
        Ok(releases)
    }
}
