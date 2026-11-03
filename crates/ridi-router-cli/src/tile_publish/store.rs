use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use super::sigv4::{self, Credentials};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{key}: {source}")]
    Io { key: String, source: std::io::Error },
    #[error("{method} {key} failed with HTTP {status}: {body}")]
    Http {
        method: &'static str,
        key: String,
        status: u16,
        body: String,
    },
    #[error("{method} {key} failed: {message}")]
    Transport {
        method: &'static str,
        key: String,
        message: String,
    },
}

pub struct PutOptions {
    pub content_type: &'static str,
    pub cache_control: &'static str,
}

/// Where a release is published: an S3-compatible bucket, or a local directory with the same
/// layout (for serving to an emulator or inspecting before an upload).
pub trait ObjectStore: Sync {
    fn describe(&self) -> String;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;
    fn exists(&self, key: &str) -> Result<bool, StoreError>;
    fn put(&self, key: &str, body: &[u8], options: &PutOptions) -> Result<(), StoreError>;
}

pub struct DirStore {
    root: PathBuf,
}

impl DirStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

impl ObjectStore for DirStore {
    fn describe(&self) -> String {
        format!("directory {}", self.root.display())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match fs::read(self.path(key)) {
            Ok(data) => Ok(Some(data)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(StoreError::Io {
                key: key.to_string(),
                source,
            }),
        }
    }

    fn exists(&self, key: &str) -> Result<bool, StoreError> {
        Ok(self.path(key).is_file())
    }

    fn put(&self, key: &str, body: &[u8], _options: &PutOptions) -> Result<(), StoreError> {
        let path = self.path(key);
        let io_error = |source| StoreError::Io {
            key: key.to_string(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        // Write then rename so a reader never sees a partial index.
        let temp = temp_path(&path);
        fs::write(&temp, body).map_err(io_error)?;
        fs::rename(&temp, &path).map_err(io_error)
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

pub struct S3Store {
    agent: ureq::Agent,
    endpoint: String,
    host: String,
    bucket: String,
    credentials: Credentials,
}

impl S3Store {
    /// `endpoint` like `https://<account_id>.r2.cloudflarestorage.com`.
    pub fn new(endpoint: &str, bucket: &str, credentials: Credentials) -> Self {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        let host = endpoint
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(&endpoint)
            .to_string();
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .timeout(Duration::from_secs(300))
                .build(),
            endpoint,
            host,
            bucket: bucket.to_string(),
            credentials,
        }
    }

    fn request(
        &self,
        method: &'static str,
        key: &str,
        body: &[u8],
        extra_headers: &[(&str, &str)],
    ) -> Result<Option<ureq::Response>, StoreError> {
        let canonical_path = sigv4::uri_encode_path(&format!("/{}/{key}", self.bucket));
        let payload_hash = sigv4::sha256_hex(body);

        let mut last_error = String::new();
        for attempt in 0..4 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(500 << attempt));
            }
            // Signed per attempt: a signature is only valid for 15 minutes around x-amz-date.
            let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
            let mut headers = vec![
                ("host".to_string(), self.host.clone()),
                ("x-amz-content-sha256".to_string(), payload_hash.clone()),
                ("x-amz-date".to_string(), amz_date.clone()),
            ];
            for (name, value) in extra_headers {
                headers.push((name.to_string(), value.to_string()));
            }
            let authorization = sigv4::authorization(
                &self.credentials,
                method,
                &canonical_path,
                &headers,
                &payload_hash,
                &amz_date,
            );
            let mut request = self
                .agent
                .request(method, &format!("{}{canonical_path}", self.endpoint))
                .set("authorization", &authorization);
            for (name, value) in &headers {
                if name != "host" {
                    request = request.set(name, value);
                }
            }
            let result = if body.is_empty() && method != "PUT" {
                request.call()
            } else {
                request.send_bytes(body)
            };
            match result {
                Ok(response) => return Ok(Some(response)),
                Err(ureq::Error::Status(404, _)) => return Ok(None),
                Err(ureq::Error::Status(status, response)) if status < 500 && status != 429 => {
                    return Err(StoreError::Http {
                        method,
                        key: key.to_string(),
                        status,
                        body: response.into_string().unwrap_or_default(),
                    });
                }
                Err(error) => last_error = error.to_string(),
            }
        }
        Err(StoreError::Transport {
            method,
            key: key.to_string(),
            message: last_error,
        })
    }
}

impl ObjectStore for S3Store {
    fn describe(&self) -> String {
        format!("bucket {} at {}", self.bucket, self.endpoint)
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(response) = self.request("GET", key, &[], &[])? else {
            return Ok(None);
        };
        let mut data = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut data)
            .map_err(|source| StoreError::Io {
                key: key.to_string(),
                source,
            })?;
        Ok(Some(data))
    }

    fn exists(&self, key: &str) -> Result<bool, StoreError> {
        Ok(self.request("HEAD", key, &[], &[])?.is_some())
    }

    fn put(&self, key: &str, body: &[u8], options: &PutOptions) -> Result<(), StoreError> {
        self.request(
            "PUT",
            key,
            body,
            &[
                ("content-type", options.content_type),
                ("cache-control", options.cache_control),
            ],
        )?;
        Ok(())
    }
}
