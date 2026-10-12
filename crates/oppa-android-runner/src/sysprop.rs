//! Android system-property switches (decision 434): diagnostics that
//! survive NativeActivity's unsettable environment. Reads go through
//! libc `__system_property_get` (stable NDK API, no new dependencies).
//! Unset or unreadable properties read as absent — never an error,
//! never a default-on.

use std::ffi::{c_char, c_int, CString};

extern "C" {
    fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
}

/// Raw string value of an Android system property (`None` when unset
/// or unreadable — never an error).
pub fn system_prop(name: &str) -> Option<String> {
    let key = CString::new(name).ok()?;
    // PROP_VALUE_MAX (92) from <sys/system_properties.h>; the call
    // NUL-terminates and returns the length excluding the NUL.
    let mut buf = [0 as c_char; 92];
    // SAFETY: `buf` is a live 92-byte backing store; libc writes at
    // most 92 bytes including the NUL and returns the payload length,
    // so the read below stays in-bounds and NUL-free.
    let len = unsafe { __system_property_get(key.as_ptr(), buf.as_mut_ptr()) };
    if len <= 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len as usize) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Boolean switch over a system property: `1`/`true` (any case,
/// trimmed) reads on; anything else — including unset — reads off.
pub fn system_prop_bool(name: &str) -> bool {
    match system_prop(name) {
        Some(v) => {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true"
        }
        None => false,
    }
}
