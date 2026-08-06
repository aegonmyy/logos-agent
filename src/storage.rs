//! Logos Storage integration for the agent (LP-0008, stage 2).
//!
//! The agent stores files privately: content is encrypted client-side before it
//! leaves the process, so the storage node only ever holds ciphertext. Objects
//! are addressed by a content address (a CID from a real Codex node, or a hash
//! in the in-memory backend used for tests).
//!
//! [`Storage`] is the seam the `storage.*` skills call. [`CodexStorage`] talks
//! to a real Logos Storage / Codex node over HTTP; [`InMemoryStorage`] is a
//! self-contained backend for deterministic tests.

use std::collections::HashMap;
use std::sync::Mutex;

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

/// A stored object's content address (a CID, or a content hash in-memory).
pub type ContentAddress = String;

/// Where the agent keeps its files. All implementations store ciphertext only.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Encrypt `data` and store it under `label`; returns its content address.
    async fn upload(&self, label: &str, data: &[u8]) -> Result<ContentAddress>;
    /// Retrieve and decrypt the object at `address`.
    async fn download(&self, address: &ContentAddress) -> Result<Vec<u8>>;
    /// List stored objects as `(label, address)` pairs.
    async fn list(&self) -> Result<Vec<(String, ContentAddress)>>;
    /// Grant `recipient` (a Logos identity) access to the object at `address`.
    async fn share(&self, address: &ContentAddress, recipient: &str) -> Result<()>;
}

/// Encrypt `plaintext` with AES-256-GCM under `key`; returns `nonce || ciphertext`.
fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|err| anyhow!("encryption failed: {err}"))?;
    let mut sealed = nonce.to_vec();
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Reverse [`seal`].
fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 12 {
        bail!("sealed object is too short to contain a nonce");
    }
    let (nonce_bytes, ciphertext) = sealed.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|err| anyhow!("decryption failed: {err}"))
}

/// A record the agent keeps for each object it has stored.
struct Stored {
    label: String,
    sealed: Vec<u8>,
    shared_with: Vec<String>,
}

/// In-memory storage backend: encrypts exactly like the real one, addresses by
/// SHA-256 of the ciphertext, and keeps everything in a map. For tests.
pub struct InMemoryStorage {
    key: [u8; 32],
    objects: Mutex<HashMap<ContentAddress, Stored>>,
}

impl InMemoryStorage {
    #[must_use]
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            objects: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    async fn upload(&self, label: &str, data: &[u8]) -> Result<ContentAddress> {
        let sealed = seal(&self.key, data)?;
        let address = hex::encode(Sha256::digest(&sealed));
        let mut objects = self.objects.lock().expect("storage lock poisoned");
        objects.insert(
            address.clone(),
            Stored {
                label: label.to_owned(),
                sealed,
                shared_with: Vec::new(),
            },
        );
        Ok(address)
    }

    async fn download(&self, address: &ContentAddress) -> Result<Vec<u8>> {
        let objects = self.objects.lock().expect("storage lock poisoned");
        let object = objects
            .get(address)
            .with_context(|| format!("no object at address {address}"))?;
        open(&self.key, &object.sealed)
    }

    async fn list(&self) -> Result<Vec<(String, ContentAddress)>> {
        let objects = self.objects.lock().expect("storage lock poisoned");
        let mut listing: Vec<(String, ContentAddress)> = objects
            .iter()
            .map(|(address, object)| (object.label.clone(), address.clone()))
            .collect();
        listing.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(listing)
    }

    async fn share(&self, address: &ContentAddress, recipient: &str) -> Result<()> {
        let mut objects = self.objects.lock().expect("storage lock poisoned");
        let object = objects
            .get_mut(address)
            .with_context(|| format!("no object at address {address}"))?;
        object.shared_with.push(recipient.to_owned());
        Ok(())
    }
}

/// Real Logos Storage backend, talking to a Codex node over HTTP. Content is
/// encrypted here before upload; a local index maps labels to the CIDs the node
/// returns (Codex is content-addressed and does not list on the agent's behalf).
pub struct CodexStorage {
    base: String,
    http: reqwest::Client,
    key: [u8; 32],
    index: Mutex<Vec<(String, ContentAddress)>>,
}

impl CodexStorage {
    /// `base` is the Codex REST endpoint, e.g. `http://127.0.0.1:8080`.
    #[must_use]
    pub fn new(base: impl Into<String>, key: [u8; 32]) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            key,
            index: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Storage for CodexStorage {
    async fn upload(&self, label: &str, data: &[u8]) -> Result<ContentAddress> {
        let sealed = seal(&self.key, data)?;
        let response = self
            .http
            .post(format!("{}/api/storage/v1/data", self.base))
            .header("Content-Type", "application/octet-stream")
            .body(sealed)
            .send()
            .await
            .context("POST /api/storage/v1/data")?
            .error_for_status()
            .context("storage node rejected upload")?;
        let cid = response.text().await.context("reading CID")?.trim().to_owned();
        self.index
            .lock()
            .expect("index lock poisoned")
            .push((label.to_owned(), cid.clone()));
        Ok(cid)
    }

    async fn download(&self, address: &ContentAddress) -> Result<Vec<u8>> {
        let sealed = self
            .http
            .get(format!(
                "{}/api/storage/v1/data/{address}/network/stream",
                self.base
            ))
            .send()
            .await
            .context("GET codex data")?
            .error_for_status()
            .context("storage node rejected download")?
            .bytes()
            .await
            .context("reading object bytes")?;
        open(&self.key, &sealed)
    }

    async fn list(&self) -> Result<Vec<(String, ContentAddress)>> {
        Ok(self.index.lock().expect("index lock poisoned").clone())
    }

    async fn share(&self, address: &ContentAddress, _recipient: &str) -> Result<()> {
        // On Codex the CID itself is the capability to read; sharing is
        // delivering the (encrypted) CID to the recipient over Messaging. The
        // caller pairs this with `messaging.send`. We validate the object exists.
        self.index
            .lock()
            .expect("index lock poisoned")
            .iter()
            .any(|(_, cid)| cid == address)
            .then_some(())
            .with_context(|| format!("unknown object {address}"))
    }
}
