//! Web key-value storage (Round 4.3): `localStorage` behind the
//! core [`KvStore`](oppa::store::KvStore) seam (G5 sync-first rule —
//! `localStorage` is synchronous on every platform, so no
//! request/poll machinery).
//!
//! Split where it is provable: [`StrStore`] abstracts the string
//! transport — [`LocalBackend`] speaks `web_sys` (compiles
//! everywhere, runs on wasm; `web_sys` traps off-wasm, so host
//! tests never touch it) and [`MemBackend`] backs the full
//! behavior matrix on host. [`BrowserKv`] owns the key rule,
//! UTF-8 rule, prefix, and error mapping over either —
//! host-tested, identical logic on device.
//!
//! Rules (stated):
//!
//! - Keys are flat non-empty strings (the core rule); stored
//!   under the `oppa:` prefix (origin-shared — multi-app
//!   safety, never a silent cross-app read).
//! - Values are UTF-8: binary refuses loudly as `Backend` (the
//!   G5 design pre-documents the UTF-8 check — `localStorage`
//!   holds strings only, no base64 smuggling, no new deps).
//! - Missing reads are `Ok(None)` (query, not failure — the
//!   clipboard empty-`Ok(None)` split).
//! - `FsSandbox` stays `Unsupported` on web (G5 out-of-scope —
//!   unchanged this round).

use std::collections::HashMap;

use oppa::store::{KvStore, StoreError};

/// String transport behind [`BrowserKv`] (what `localStorage`
/// does, minus the browser).
pub trait StrStore {
    fn str_get(&self, key: &str) -> Result<Option<String>, String>;
    fn str_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn str_remove(&mut self, key: &str) -> Result<(), String>;
    fn str_clear(&mut self) -> Result<(), String>;
}

/// `web_sys` Storage adapter (the 6-line review-only surface —
/// every rule above it is host-proven through [`MemBackend`]).
pub struct LocalBackend {
    storage: web_sys::Storage,
}

impl LocalBackend {
    /// Opens the window's `localStorage` (`None` in privacy mode
    /// / no window — the caller falls back to memory, stated in
    /// the binding). Host builds return `None` by compile-time
    /// gate (`web_sys` traps off-wasm instead of returning
    /// `None` — the DPR precedent, never a runtime guess).
    pub fn open() -> Option<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            let storage = web_sys::window()?.local_storage().ok()??;
            Some(Self { storage })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }
}

impl StrStore for LocalBackend {
    fn str_get(&self, key: &str) -> Result<Option<String>, String> {
        self.storage
            .get_item(key)
            .map_err(|e| format!("localStorage.get: {e:?}"))
    }

    fn str_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.storage
            .set_item(key, value)
            .map_err(|e| format!("localStorage.set: {e:?}"))
    }

    fn str_remove(&mut self, key: &str) -> Result<(), String> {
        self.storage
            .remove_item(key)
            .map_err(|e| format!("localStorage.remove: {e:?}"))
    }

    fn str_clear(&mut self) -> Result<(), String> {
        self.storage
            .clear()
            .map_err(|e| format!("localStorage.clear: {e:?}"))
    }
}

/// In-memory transport (host/test twin — proves every
/// [`BrowserKv`] rule without a browser).
#[derive(Clone, Debug, Default)]
pub struct MemBackend {
    map: HashMap<String, String>,
}
impl MemBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StrStore for MemBackend {
    fn str_get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.map.get(key).cloned())
    }

    fn str_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.map.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn str_remove(&mut self, key: &str) -> Result<(), String> {
        self.map.remove(key);
        Ok(())
    }

    fn str_clear(&mut self) -> Result<(), String> {
        self.map.clear();
        Ok(())
    }
}

/// `localStorage`-shaped [`KvStore`] over any [`StrStore`].
pub struct BrowserKv<S: StrStore> {
    backend: S,
}

impl<S: StrStore> BrowserKv<S> {
    pub fn new(backend: S) -> Self {
        Self { backend }
    }

    /// Storage key on the wire (`oppa:`-prefixed — never bare).
    pub fn wire_key(key: &str) -> String {
        format!("oppa:{key}")
    }
}

fn check_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty() {
        return Err(StoreError::InvalidKey(key.to_string()));
    }
    Ok(())
}

impl<S: StrStore> KvStore for BrowserKv<S> {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        check_key(key)?;
        self.backend
            .str_get(&Self::wire_key(key))
            .map_err(StoreError::Backend)
            .map(|opt| opt.map(String::into_bytes))
    }

    fn set(&mut self, key: &str, value: Vec<u8>) -> Result<(), StoreError> {
        check_key(key)?;
        let text = String::from_utf8(value).map_err(|_| {
            StoreError::Backend(format!(
                "value for key {key:?} is not UTF-8 — localStorage holds strings only"
            ))
        })?;
        self.backend
            .str_set(&Self::wire_key(key), &text)
            .map_err(StoreError::Backend)
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        check_key(key)?;
        self.backend
            .str_remove(&Self::wire_key(key))
            .map_err(StoreError::Backend)
    }

    fn clear(&mut self) -> Result<(), StoreError> {
        // Clears the whole origin storage (documented scope —
        // prefix-scoped clear would need key iteration, and
        // `key(i)` enumeration races concurrent writers; the demo
        // owns its origin on the raw server, stated).
        self.backend.str_clear().map_err(StoreError::Backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv() -> BrowserKv<MemBackend> {
        BrowserKv::new(MemBackend::new())
    }

    /// Round 4.3: the full backend matrix through the memory twin
    /// (identical logic on device — only the 6-line adapter
    /// differs, review-only).
    #[test]
    fn round_trip_remove_clear_and_miss() {
        let mut kv = kv();
        assert_eq!(kv.get("theme").expect("reads"), None);
        kv.set("theme", b"dark".to_vec()).expect("writes");
        assert_eq!(kv.get("theme").expect("reads"), Some(b"dark".to_vec()));
        kv.remove("theme").expect("removes");
        assert_eq!(kv.get("theme").expect("reads"), None);
        kv.set("a", b"1".to_vec()).expect("writes");
        kv.clear().expect("clears");
        assert_eq!(kv.get("a").expect("reads"), None);
    }

    #[test]
    fn empty_keys_refuse_and_prefix_namespaces() {
        let mut kv = kv();
        assert_eq!(
            kv.set("", b"x".to_vec()),
            Err(StoreError::InvalidKey(String::new()))
        );
        assert_eq!(kv.get(""), Err(StoreError::InvalidKey(String::new())));
        assert_eq!(BrowserKv::<MemBackend>::wire_key("theme"), "oppa:theme");
        kv.set("theme", b"dark".to_vec()).expect("writes");
        assert_eq!(
            kv.backend.map.get("oppa:theme").map(String::as_str),
            Some("dark"),
            "wire form is prefixed, never bare"
        );
        assert!(!kv.backend.map.contains_key("theme"), "no unprefixed leak");
    }

    #[test]
    fn binary_values_refuse_loudly() {
        let mut kv = kv();
        let err = kv
            .set("blob", vec![0xFF, 0xFE])
            .expect_err("binary refuses");
        assert!(
            err.to_string().contains("not UTF-8"),
            "loud UTF-8 rule, got {err}"
        );
        assert_eq!(
            kv.get("blob").expect("reads"),
            None,
            "refused write stores nothing"
        );
    }
}
