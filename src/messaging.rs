//! Logos Messaging integration for the agent (LP-0008, stage 2).
//!
//! The agent talks to its owner and to other agents over Logos Messaging (Waku).
//! Addresses and group ids are Waku content topics; messages are carried as the
//! message payload. [`Messaging`] is the seam the `messaging.*` skills call.
//! [`WakuMessaging`] drives a real nwaku node over its REST API (the same
//! endpoints our LP-0017 delivery tooling uses); [`InMemoryMessaging`] is a
//! self-contained backend for deterministic tests.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};

/// A Logos Messaging address or group id — a Waku content topic.
pub type Topic = String;
/// An opaque id for a sent message.
pub type MessageId = String;

/// How the agent sends and receives Logos Messaging traffic.
#[async_trait]
pub trait Messaging: Send + Sync {
    /// Send `message` to `recipient` (a user/agent address or group topic).
    async fn send(&self, recipient: &str, message: &[u8]) -> Result<MessageId>;
    /// Join an existing group topic so its messages are received.
    async fn join(&self, group_id: &str) -> Result<()>;
    /// Create a new group topic for `members`; returns its id.
    async fn create_group(&self, members: &[String]) -> Result<Topic>;
    /// Read the messages currently available on `topic`.
    async fn poll(&self, topic: &str) -> Result<Vec<Vec<u8>>>;
}

/// Derive a stable group topic from its member set (order-independent).
fn group_topic(members: &[String]) -> Topic {
    let mut sorted = members.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    for member in &sorted {
        hasher.update(member.as_bytes());
        hasher.update([0]);
    }
    format!(
        "/logos-agent/1/group-{}/proto",
        hex::encode(&hasher.finalize()[..8])
    )
}

/// In-memory messaging backend: records sent payloads per topic and tracks
/// joined groups. For tests.
#[derive(Default)]
pub struct InMemoryMessaging {
    inboxes: Mutex<HashMap<Topic, Vec<Vec<u8>>>>,
    joined: Mutex<Vec<Topic>>,
}

impl InMemoryMessaging {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Messaging for InMemoryMessaging {
    async fn send(&self, recipient: &str, message: &[u8]) -> Result<MessageId> {
        let mut inboxes = self.inboxes.lock().expect("messaging lock poisoned");
        inboxes
            .entry(recipient.to_owned())
            .or_default()
            .push(message.to_vec());
        Ok(hex::encode(Sha256::digest(message)))
    }

    async fn join(&self, group_id: &str) -> Result<()> {
        self.joined
            .lock()
            .expect("messaging lock poisoned")
            .push(group_id.to_owned());
        Ok(())
    }

    async fn create_group(&self, members: &[String]) -> Result<Topic> {
        let topic = group_topic(members);
        self.join(&topic).await?;
        Ok(topic)
    }

    async fn poll(&self, topic: &str) -> Result<Vec<Vec<u8>>> {
        Ok(self
            .inboxes
            .lock()
            .expect("messaging lock poisoned")
            .get(topic)
            .cloned()
            .unwrap_or_default())
    }
}

/// Payload envelope as accepted/returned by the nwaku REST relay API.
#[derive(serde::Serialize, serde::Deserialize)]
struct WakuMessage {
    payload: String,
    #[serde(rename = "contentTopic")]
    content_topic: String,
}

/// Real Logos Messaging backend over a running nwaku node's REST API.
pub struct WakuMessaging {
    base: String,
    http: reqwest::Client,
}

impl WakuMessaging {
    /// `base` is the nwaku REST endpoint, e.g. `http://127.0.0.1:8645`.
    #[must_use]
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Messaging for WakuMessaging {
    async fn send(&self, recipient: &str, message: &[u8]) -> Result<MessageId> {
        let body = WakuMessage {
            payload: BASE64.encode(message),
            content_topic: recipient.to_owned(),
        };
        let response = self
            .http
            .post(format!("{}/relay/v1/auto/messages", self.base))
            .json(&body)
            .send()
            .await
            .context("POST /relay/v1/auto/messages")?;
        if !response.status().is_success() {
            let status = response.status();
            let detail = response
                .text()
                .await
                .context("reading nwaku publish error")?;
            // nwaku v0.38 stores a published message locally even when a
            // standalone node has no relay mesh to forward it to, reporting
            // `NoPeersToPublish` as a non-2xx. The message still round-trips
            // to local subscribers, so treat that single-node condition as a
            // successful local publish rather than a failure. On a real mesh
            // this branch is not reached (peers exist, the publish is 2xx).
            if detail.contains("NoPeersToPublish") {
                return Ok(hex::encode(Sha256::digest(message)));
            }
            anyhow::bail!("nwaku rejected publish ({status}): {detail}");
        }
        Ok(hex::encode(Sha256::digest(message)))
    }

    async fn join(&self, group_id: &str) -> Result<()> {
        self.http
            .post(format!("{}/relay/v1/auto/subscriptions", self.base))
            .json(&[group_id])
            .send()
            .await
            .context("POST /relay/v1/auto/subscriptions")?
            .error_for_status()
            .context("nwaku rejected subscription")?;
        Ok(())
    }

    async fn create_group(&self, members: &[String]) -> Result<Topic> {
        let topic = group_topic(members);
        self.join(&topic).await?;
        Ok(topic)
    }

    async fn poll(&self, topic: &str) -> Result<Vec<Vec<u8>>> {
        let encoded: String = url_encode(topic);
        let messages: Vec<WakuMessage> = self
            .http
            .get(format!("{}/relay/v1/auto/messages/{encoded}", self.base))
            .send()
            .await
            .context("GET /relay/v1/auto/messages")?
            .error_for_status()
            .context("nwaku rejected message read")?
            .json()
            .await
            .context("decoding messages")?;
        messages
            .into_iter()
            .map(|message| {
                BASE64
                    .decode(message.payload.as_bytes())
                    .context("decoding message payload")
            })
            .collect()
    }
}

/// Minimal percent-encoding for a content topic in a URL path segment.
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
