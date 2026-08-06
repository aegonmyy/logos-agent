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

use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::messaging::InMemoryMessaging;
use crate::skills::SkillRegistry;
use crate::storage::InMemoryStorage;

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

#[cfg(test)]
mod tests {
    use super::default_catalogue;

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
}
