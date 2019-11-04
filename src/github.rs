use chrono::{DateTime, Utc};
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{self, Result};
use std::collections::HashMap;

const GITHUB_API_URL: &str = "https://api.github.com";

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
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn new(username: String, password: String) -> Credentials {
        Credentials { username, password }
    }
}

pub struct EtagCache {
    pub hash: String,
    pub content: serde_json::Value,
}

impl EtagCache {
    pub fn new(hash: String, content: serde_json::Value) -> EtagCache {
        EtagCache { hash, content }
    }
}

pub struct GitHub {
    creds: Option<Credentials>,
    etags: HashMap<String, EtagCache>,
    client: Client,
}

impl GitHub {
    pub fn new() -> GitHub {
        GitHub {
            creds: None,
            etags: HashMap::new(),
            client: Client::new(),
        }
    }

    pub fn with_creds(creds: Credentials) -> GitHub {
        GitHub {
            creds: Some(creds),
            etags: HashMap::new(),
            client: Client::new(),
        }
    }

    fn cached_get(&mut self, endpoint: &str) -> Result<serde_json::Value> {
        let req = self.client.get(&format!("{}/{}", GITHUB_API_URL, endpoint));

        // Inject the username and password if provided
        let req = match self.creds {
            Some(ref creds) => {
                req.basic_auth(&creds.username, Some(&creds.password))
            }
            None => req,
        };

        // Inject in etag if available
        let req = match self.etags.get(endpoint) {
            Some(cache) => req.header(header::IF_NONE_MATCH, &cache.hash),
            None => req,
        };

        // Be careful with: https://developer.github.com/v3/#rate-limiting
        // Also see: https://developer.github.com/v3/#conditional-requests
        let mut rsp = req.send().expect("Send error");

        let content = match rsp.status() {
            StatusCode::OK => {
                let content: serde_json::Value =
                    rsp.json().expect("JSON conversion error");

                if let Some(etag) = rsp.headers().get(header::ETAG) {
                    let hash = etag
                        .to_str()
                        .expect("ETAG string conversion error")
                        .to_owned();

                    self.etags.insert(
                        endpoint.to_owned(),
                        EtagCache::new(hash, content.clone()),
                    );
                }

                content
            }

            StatusCode::NOT_MODIFIED => {
                // println!("Not modified!");

                let content = match self.etags.get(endpoint) {
                    Some(cache) => cache.content.clone(),
                    None => unimplemented!(), // this should not happen
                };

                content
            }

            _ => unimplemented!(),
        };

        Ok(content)
    }

    pub fn releases(&mut self, owner: &str, repo: &str) -> Result<Releases> {
        let endpoint = format!("repos/{}/{}/releases", owner, repo);
        let content = self.cached_get(&endpoint).expect("Content error");
        let releases: Releases = serde_json::from_value(content)?;

        Ok(releases)
    }
}
