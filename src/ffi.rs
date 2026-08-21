//! C ABI for embedding the agent in a Logos Core module (LP-0008, stage 4/C).
//!
//! The Logos Core module (C++/Qt) links this crate as a `cdylib` and calls these
//! functions with JSON in / JSON out — the same bridging pattern the chronicle
//! module uses. Strings returned here are owned by the caller until handed back
//! to [`logos_agent_free_string`].
//!
//! The stateless describe/version calls below are the ones the module needs to
//! enumerate the agent's capabilities to the app; the live agent lifecycle
//! (create account, invoke on-chain skills) is driven through the crate's Rust
//! API from the module, against a running Logos Core wallet.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use lee::AccountId;
use serde_json::{Value, json};
use wallet::WalletCore;

use crate::messaging::{InMemoryMessaging, Messaging, WakuMessaging};
use crate::owner::OwnerChannel;
use crate::skills::{SkillContext, SkillRegistry};
use crate::storage::InMemoryStorage;
use crate::{Agent, SpendingPolicy};

/// Move a Rust `String` into a C-owned string.
fn to_cstring(text: String) -> *mut c_char {
    CString::new(text)
        .unwrap_or_else(|_| {
            CString::new(r#"{"ok":false,"error":"null byte in output"}"#)
                .expect("static json has no null byte")
        })
        .into_raw()
}

/// Free a string previously returned by this library.
///
/// # Safety
/// `ptr` must be a pointer returned by one of this module's functions, and must
/// not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

/// Return the library version as `{"ok":true,"version":"..."}`.
#[unsafe(no_mangle)]
pub extern "C" fn logos_agent_version() -> *mut c_char {
    to_cstring(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }).to_string())
}

/// Return the catalogue of default skills the agent can perform, as JSON. The
/// module surfaces this to the Logos app so the owner sees what the agent offers.
#[unsafe(no_mangle)]
pub extern "C" fn logos_agent_default_skills() -> *mut c_char {
    to_cstring(default_catalogue().to_string())
}

fn default_catalogue() -> Value {
    let mut registry = SkillRegistry::with_defaults();
    registry.register_storage(Arc::new(InMemoryStorage::new([0u8; 32])) as Arc<_>);
    registry.register_messaging(Arc::new(InMemoryMessaging::new()) as Arc<_>);
    json!({ "ok": true, "skills": registry.catalogue() })
}

// --- stateful agent session --------------------------------------------------

/// A live agent the module drives: its identity, skills, an async runtime, and —
/// when running inside Logos Core — a wallet for on-chain skills. Held across FFI
/// calls behind an opaque handle.
pub struct AgentSession {
    runtime: tokio::runtime::Runtime,
    agent: Agent,
    registry: SkillRegistry,
    wallet: Option<WalletCore>,
}

impl AgentSession {
    /// A session with in-memory Storage/Messaging and no wallet: enough to run
    /// the reflective, storage, and messaging skills. The wallet-backed
    /// on-chain skills are wired the same way when a `WalletCore` is provided.
    fn new_offline(account_id: &str) -> Result<Self> {
        let account_id: AccountId = account_id
            .parse()
            .map_err(|_| anyhow!("invalid account id"))?;
        let agent = Agent::from_parts(
            account_id,
            SpendingPolicy {
                per_tx_limit: 0,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        );
        let mut registry = SkillRegistry::with_defaults();
        registry.register_storage(Arc::new(InMemoryStorage::new([0u8; 32])) as Arc<_>);
        registry.register_messaging(Arc::new(InMemoryMessaging::new()) as Arc<_>);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to start runtime: {err}"))?;
        Ok(Self {
            runtime,
            agent,
            registry,
            wallet: None,
        })
    }

    /// Invoke a skill by name with JSON args, blocking on the runtime.
    fn invoke(&mut self, name: &str, args: Value) -> Result<Value> {
        let Self {
            runtime,
            agent,
            registry,
            wallet,
        } = self;
        runtime.block_on(async {
            let mut ctx = SkillContext {
                wallet: wallet.as_mut(),
                agent,
            };
            registry.dispatch(name, &mut ctx, args).await
        })
    }
}

/// Read a C string into an owned `String`, or `None` if null/invalid UTF-8.
///
/// # Safety
/// `ptr` must be null or a valid NUL-terminated C string.
unsafe fn read_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

/// Create an offline agent session for `account_id`. Returns null on error; the
/// handle must be freed with [`logos_agent_session_free`].
///
/// # Safety
/// `account_id` must be null or a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_session_new_offline(
    account_id: *const c_char,
) -> *mut AgentSession {
    let Some(id) = (unsafe { read_cstr(account_id) }) else {
        return std::ptr::null_mut();
    };
    match AgentSession::new_offline(&id) {
        Ok(session) => Box::into_raw(Box::new(session)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Invoke skill `name` on `session` with `args_json`, returning a JSON string
/// `{"ok":true,"result":...}` or `{"ok":false,"error":...}`.
///
/// # Safety
/// `session` must be a handle from [`logos_agent_session_new_offline`]; `name`
/// and `args_json` must be null or valid C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_session_invoke(
    session: *mut AgentSession,
    name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return to_cstring(json!({ "ok": false, "error": "null session" }).to_string());
    };
    let Some(name) = (unsafe { read_cstr(name) }) else {
        return to_cstring(json!({ "ok": false, "error": "null skill name" }).to_string());
    };
    let args: Value = unsafe { read_cstr(args_json) }
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}));

    let response = match session.invoke(&name, args) {
        Ok(result) => json!({ "ok": true, "result": result }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };
    to_cstring(response.to_string())
}

/// Destroy an agent session handle.
///
/// # Safety
/// `session` must be a handle from [`logos_agent_session_new_offline`] not
/// already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_session_free(session: *mut AgentSession) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

// --- owner channel (Basecamp owner app) -------------------------------------

/// The owner side of an agent's owner channel, held by the Basecamp owner app
/// (a separate Logos app instance) so it can read the agent's approval requests
/// and reply over Logos Messaging — with no intermediary server. The agent side
/// lives in the headless `agent` binary (or the agent module); the two talk over
/// a Logos Messaging node (Waku), each deriving the same two topics from the
/// `(agent, owner)` pair.
pub struct OwnerChannelHandle {
    runtime: tokio::runtime::Runtime,
    channel: OwnerChannel,
}

impl OwnerChannelHandle {
    /// Open the owner channel against a Logos Messaging node. A non-empty
    /// `messaging_url` is a Waku REST endpoint (the same URL the headless agent
    /// uses); an empty URL falls back to an in-memory backend, which is only
    /// useful for tests (it does not cross processes).
    pub fn new(messaging_url: &str, agent_id: &str, owner: &str) -> Result<Self> {
        let messaging: Arc<dyn Messaging> = if messaging_url.is_empty() {
            Arc::new(InMemoryMessaging::new())
        } else {
            Arc::new(WakuMessaging::new(messaging_url))
        };
        Self::from_messaging(messaging, agent_id, owner)
    }

    /// Build the handle from an existing messaging backend (used by tests so the
    /// agent and owner sides can share one in-memory backend).
    pub fn from_messaging(
        messaging: Arc<dyn Messaging>,
        agent_id: &str,
        owner: &str,
    ) -> Result<Self> {
        let account_id: AccountId = agent_id
            .parse()
            .map_err(|_| anyhow!("invalid agent account id"))?;
        let channel = OwnerChannel::open(messaging, &account_id, owner);
        // enable_all, not just time: a Waku-backed channel does real socket IO
        // inside block_on, and a runtime without an IO driver panics on the
        // first poll with "IO is disabled".
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to start runtime: {err}"))?;
        Ok(Self { runtime, channel })
    }

    /// Owner reads the agent's pending approval requests (`agent.poll`).
    pub fn poll(&self) -> Result<Vec<Value>> {
        self.runtime.block_on(self.channel.poll_agent_requests())
    }

    /// Owner approves or denies a pending request by id (`agent.decide`).
    pub fn decide(&self, request_id: &str, approve: bool) -> Result<()> {
        self.runtime
            .block_on(self.channel.decide(request_id, approve))
    }

    /// Owner sets the per-transaction spending limit (`agent.configure`).
    pub fn configure_limit(&self, per_tx_limit: u128) -> Result<()> {
        self.runtime
            .block_on(self.channel.configure_limit(per_tx_limit))
    }

    /// Owner sets the per-period spending policy (`agent.configure_period`).
    pub fn configure_period(&self, per_period_limit: u128, period_seconds: u64) -> Result<()> {
        self.runtime
            .block_on(self.channel.configure_period(per_period_limit, period_seconds))
    }

    /// Parse a decimal limit string (C-ABI-safe: u128 has no portable C type).
    fn parse_limit(text: &str) -> Result<u128> {
        text.trim()
            .parse::<u128>()
            .map_err(|_| anyhow!("limit must be a non-negative decimal integer"))
    }
}

/// Open an owner channel for `agent_id` talking to `owner` over the Logos
/// Messaging node at `messaging_url` (Waku REST). Returns null on error; free
/// the handle with [`logos_agent_owner_channel_free`]. An empty `messaging_url`
/// uses an in-memory backend (tests only — does not cross processes).
///
/// # Safety
/// `messaging_url`, `agent_id`, and `owner` must be null or valid C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_channel_new(
    messaging_url: *const c_char,
    agent_id: *const c_char,
    owner: *const c_char,
) -> *mut OwnerChannelHandle {
    let Some(url) = (unsafe { read_cstr(messaging_url) }) else {
        return std::ptr::null_mut();
    };
    let Some(agent) = (unsafe { read_cstr(agent_id) }) else {
        return std::ptr::null_mut();
    };
    let Some(owner) = (unsafe { read_cstr(owner) }) else {
        return std::ptr::null_mut();
    };
    match OwnerChannelHandle::new(&url, &agent, &owner) {
        Ok(handle) => Box::into_raw(Box::new(handle)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Read pending approval requests: `{"ok":true,"requests":[...]}` (each request
/// is the JSON the agent posted) or `{"ok":false,"error":...}`.
///
/// # Safety
/// `handle` must be a handle from [`logos_agent_owner_channel_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_poll(
    handle: *mut OwnerChannelHandle,
) -> *mut c_char {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return to_cstring(json!({ "ok": false, "error": "null owner channel" }).to_string());
    };
    match handle.poll() {
        Ok(requests) => to_cstring(json!({ "ok": true, "requests": requests }).to_string()),
        Err(error) => to_cstring(json!({ "ok": false, "error": error.to_string() }).to_string()),
    }
}

/// Approve (`approve` true) or deny a pending request by id. Returns
/// `{"ok":true}` or `{"ok":false,"error":...}`.
///
/// # Safety
/// `handle` must be a handle from [`logos_agent_owner_channel_new`]; `request_id`
/// must be null or a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_decide(
    handle: *mut OwnerChannelHandle,
    request_id: *const c_char,
    approve: bool,
) -> *mut c_char {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return to_cstring(json!({ "ok": false, "error": "null owner channel" }).to_string());
    };
    let Some(id) = (unsafe { read_cstr(request_id) }) else {
        return to_cstring(json!({ "ok": false, "error": "null request id" }).to_string());
    };
    match handle.decide(&id, approve) {
        Ok(()) => to_cstring(json!({ "ok": true }).to_string()),
        Err(error) => to_cstring(json!({ "ok": false, "error": error.to_string() }).to_string()),
    }
}

/// Set the agent's per-transaction spending limit. `per_tx_limit` is a decimal
/// string (token amounts exceed 64 bits, and `u128` has no portable C type).
/// Returns `{"ok":true}` or `{"ok":false,"error":...}`.
///
/// # Safety
/// `handle` must be a handle from [`logos_agent_owner_channel_new`];
/// `per_tx_limit` must be null or a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_configure_limit(
    handle: *mut OwnerChannelHandle,
    per_tx_limit: *const c_char,
) -> *mut c_char {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return to_cstring(json!({ "ok": false, "error": "null owner channel" }).to_string());
    };
    let Some(limit) = (unsafe { read_cstr(per_tx_limit) }) else {
        return to_cstring(json!({ "ok": false, "error": "null limit" }).to_string());
    };
    let result = OwnerChannelHandle::parse_limit(&limit)
        .and_then(|limit| handle.configure_limit(limit));
    match result {
        Ok(()) => to_cstring(json!({ "ok": true }).to_string()),
        Err(error) => to_cstring(json!({ "ok": false, "error": error.to_string() }).to_string()),
    }
}

/// Set the agent's per-period spending policy. `per_period_limit` is a decimal
/// string; `period_seconds` is a plain 64-bit integer. Returns `{"ok":true}` or
/// `{"ok":false,"error":...}`.
///
/// # Safety
/// `handle` must be a handle from [`logos_agent_owner_channel_new`];
/// `per_period_limit` must be null or a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_configure_period(
    handle: *mut OwnerChannelHandle,
    per_period_limit: *const c_char,
    period_seconds: u64,
) -> *mut c_char {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return to_cstring(json!({ "ok": false, "error": "null owner channel" }).to_string());
    };
    let Some(limit) = (unsafe { read_cstr(per_period_limit) }) else {
        return to_cstring(json!({ "ok": false, "error": "null limit" }).to_string());
    };
    let result = OwnerChannelHandle::parse_limit(&limit)
        .and_then(|limit| handle.configure_period(limit, period_seconds));
    match result {
        Ok(()) => to_cstring(json!({ "ok": true }).to_string()),
        Err(error) => to_cstring(json!({ "ok": false, "error": error.to_string() }).to_string()),
    }
}

/// Destroy an owner channel handle.
///
/// # Safety
/// `handle` must be a handle from [`logos_agent_owner_channel_new`] not already
/// freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logos_agent_owner_channel_free(handle: *mut OwnerChannelHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentSession, OwnerChannelHandle, default_catalogue};
    use crate::messaging::InMemoryMessaging;
    use crate::owner::{AgentRuntime, OwnerChannel};
    use crate::{Agent, SpendingPolicy};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn catalogue_lists_every_category() {
        let catalogue = default_catalogue();
        let names: Vec<String> = catalogue["skills"]
            .as_array()
            .expect("skills array")
            .iter()
            .map(|skill| skill["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        for expected in [
            "wallet.send",
            "wallet.history",
            "program.deploy",
            "storage.upload",
            "messaging.send",
            "meta.configure",
        ] {
            assert!(
                names.contains(&expected.to_owned()),
                "catalogue missing {expected}"
            );
        }
    }

    #[test]
    fn offline_session_invokes_skills() {
        let mut session =
            AgentSession::new_offline("Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE").unwrap();

        // Reflection works.
        let skills = session.invoke("meta.skills", json!({})).unwrap();
        assert!(
            skills
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["name"] == "storage.upload")
        );

        // A storage round-trip through the session proves invoke wiring + the
        // in-memory backend + encryption.
        let uploaded = session
            .invoke(
                "storage.upload",
                json!({ "label": "note", "data": "hello" }),
            )
            .unwrap();
        let address = uploaded["address"].as_str().unwrap().to_owned();
        let downloaded = session
            .invoke("storage.download", json!({ "address": address }))
            .unwrap();
        assert_eq!(downloaded["data"], "hello");

        // meta.configure changes the live limit through the session.
        session
            .invoke(
                "meta.configure",
                json!({ "key": "per_tx_limit", "value": 42 }),
            )
            .unwrap();
        let status = session.invoke("meta.status", json!({})).unwrap();
        assert_eq!(status["per_tx_limit"], "42");
    }

    /// The owner side (as the Basecamp app would use it) sees the agent's
    /// approval request and can reply, over a shared messaging backend — the
    /// same flow that runs over Waku in a real deployment. This is a plain
    /// (non-async) test because the FFI handle drives its own runtime, the way
    /// the C/Qt caller does; nesting it under `#[tokio::test]` would panic.
    #[test]
    fn owner_channel_round_trips_over_messaging() {
        let messaging = Arc::new(InMemoryMessaging::new());
        let account_id: lee::AccountId =
            "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE".parse().unwrap();

        // Agent side: an over-limit spend is held and a request is posted.
        let agent = Agent::from_parts(
            account_id.clone(),
            SpendingPolicy {
                per_tx_limit: 10,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        );
        let agent_channel = OwnerChannel::open(
            Arc::clone(&messaging) as Arc<_>,
            &account_id,
            "owner",
        );
        let mut runtime = AgentRuntime::new(agent, agent_channel);
        let agent_rt = tokio::runtime::Runtime::new().unwrap();
        let decision = agent_rt
            .block_on(async {
                runtime.propose_send_no_wallet(account_id.clone(), 50).await
            })
            .unwrap();
        let request_id = match decision {
            crate::owner::SpendDecision::Pending { id } => id,
            other => panic!("expected pending, got {other:?}"),
        };

        // Owner side (the Basecamp app's handle): polls and sees the request.
        let owner = OwnerChannelHandle::from_messaging(
            Arc::clone(&messaging) as Arc<_>,
            &account_id.to_string(),
            "owner",
        )
        .unwrap();
        let requests = owner.poll().unwrap();
        let seen = requests
            .iter()
            .find(|req| req["id"].as_str() == Some(request_id.as_str()))
            .expect("owner sees the agent's approval request");
        assert_eq!(seen["amount"], "50");

        // The owner replies, and reconfigures both limits — all over messaging.
        owner.decide(&request_id, true).unwrap();
        owner.configure_limit(99).unwrap();
        owner.configure_period(500, 86_400).unwrap();

        // The agent side receives the decision and the configuration changes.
        // (No wallet here, so read the owner→agent messages without applying.)
        let messages = agent_rt
            .block_on(async { runtime.peek_owner_messages().await })
            .unwrap();
        let kinds: Vec<&str> = messages
            .iter()
            .filter_map(|m| m["type"].as_str())
            .collect();
        assert!(kinds.contains(&"decision"));
        assert!(kinds.contains(&"configure"));
        assert!(kinds.contains(&"configure_period"));
    }
}
