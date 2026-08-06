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

use crate::messaging::InMemoryMessaging;
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
        let agent = Agent::from_parts(account_id, SpendingPolicy { per_tx_limit: 0 });
        let mut registry = SkillRegistry::with_defaults();
        registry.register_storage(Arc::new(InMemoryStorage::new([0u8; 32])) as Arc<_>);
        registry.register_messaging(Arc::new(InMemoryMessaging::new()) as Arc<_>);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
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
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
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

#[cfg(test)]
mod tests {
    use super::{AgentSession, default_catalogue};
    use serde_json::json;

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
            assert!(names.contains(&expected.to_owned()), "catalogue missing {expected}");
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
            .invoke("storage.upload", json!({ "label": "note", "data": "hello" }))
            .unwrap();
        let address = uploaded["address"].as_str().unwrap().to_owned();
        let downloaded = session
            .invoke("storage.download", json!({ "address": address }))
            .unwrap();
        assert_eq!(downloaded["data"], "hello");

        // meta.configure changes the live limit through the session.
        session
            .invoke("meta.configure", json!({ "key": "per_tx_limit", "value": 42 }))
            .unwrap();
        let status = session.invoke("meta.status", json!({})).unwrap();
        assert_eq!(status["per_tx_limit"], "42");
    }
}
