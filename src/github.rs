use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Result;

pub type Releases = Vec<Release>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Release {
    pub url: String,
    pub id: u32,
    pub tag_name: String,
    pub created_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    pub id: u32,
    pub name: String,
    pub content_type: String, // application/octet-stream
    pub size: u64,
    pub browser_download_url: String,
}

pub struct Credentials {
    pub user: String,
    pub password: String,
}

impl Credentials {
    pub fn new(user: String, password: String) -> Credentials {
        Credentials { user, password }
    }
}

pub struct GitHub {
    creds: Option<Credentials>,
}

impl GitHub {
    pub fn new() -> GitHub {
        GitHub { creds: None }
    }

    pub fn with_creds(creds: Credentials) -> GitHub {
        GitHub { creds: Some(creds) }
    }

    pub fn releases(&self, owner: &str, repo: &str) -> Result<Releases> {
        let body = reqwest::get(&format!(
            "https://api.github.com/repos/{}/{}/releases",
            owner, repo
        ))
        .unwrap()
        .text()
        .unwrap();

        let releases = serde_json::from_str(&body)?;
        Ok(releases)
    }
}
