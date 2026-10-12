//! On-device picker validation (decision 424): MediaStore
//! round-trip + opener-rendezvous routing with real
//! `ContentResolver` bytes.
//!
//! Split where provable: option mapping, generation gating, and
//! dismissal/level-trigger rules live host-tested in
//! `oppa-shell-android::picker`; this module holds only what needs
//! a VM — insert/write/read/delete through `ContentResolver` —
//! plus the one device entry ([`validate_picker`]) that feeds
//! device bytes into the opener rendezvous and records the
//! outcome.
//!
//! No picker UI gesture here (that needs a human on DocumentsUI —
//! manual, recorded in `STATE.md`): the round-trip exercises the
//! exact read path SAF open will use
//! (`ContentResolver.openInputStream` on a content URI) without
//! the chooser. Fixture bytes are deterministic ASCII (no literal
//! non-ASCII in source — the corpus rule).

use std::path::Path;

use android_activity::AndroidApp;
use oppa::{FileOpener, FilePickerOptions, PickedFile};
use oppa_shell_android::picker::AndroidFileOpener;

use oppa_android_runner::ime_bridge::with_attached;

/// Round-trip file name (stable — reruns auto-rename on collision
/// and the returned URI is what the proof follows, never this
/// spelling).
const FIXTURE_NAME: &str = "oppa-picker-proof.bin";

/// Deterministic fixture bytes (ASCII only — letters, digits,
/// punctuation, newlines; sized for one 4 KiB read).
const FIXTURE: &[u8] = b"oppa-picker-proof v1\n0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\n~!@#$%^&*()_+-=[]{}|;':,./<>?\nThe quick brown fox jumps over the lazy dog 0123456789.\nSecond line for newline fidelity.\n";

/// Reads the `ContentResolver` off the activity (every resolver
/// call below fans out from here — one lookup, never per-call).
fn resolver<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'_>,
) -> Result<jni::objects::JObject<'a>, String> {
    let resolver = env
        .call_method(
            activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .map_err(|e| format!("getContentResolver: {e}"))?
        .l()
        .map_err(|e| format!("resolver obj: {e}"))?;
    if resolver.is_null() {
        return Err("ContentResolver is null".to_string());
    }
    Ok(resolver)
}

/// Inserts a Downloads entry and returns its content URI (null
/// insert fails loudly — a null URI is a platform fact the proof
/// must see, never an empty string downstream).
fn insert_entry<'a>(
    env: &mut jni::JNIEnv<'a>,
    resolver: &jni::objects::JObject<'_>,
    name: &str,
) -> Result<jni::objects::JObject<'a>, String> {
    let base = env
        .get_static_field(
            "android/provider/MediaStore$Downloads",
            "EXTERNAL_CONTENT_URI",
            "Landroid/net/Uri;",
        )
        .map_err(|e| format!("Downloads.EXTERNAL_CONTENT_URI: {e}"))?
        .l()
        .map_err(|e| format!("downloads uri: {e}"))?;
    let values = env
        .new_object("android/content/ContentValues", "()V", &[])
        .map_err(|e| format!("new ContentValues: {e}"))?;
    for (k, v) in [
        ("_display_name", name),
        ("mime_type", "application/octet-stream"),
        ("relative_path", "Download/"),
    ] {
        let kj = env.new_string(k).map_err(|e| format!("key string: {e}"))?;
        let vj = env
            .new_string(v)
            .map_err(|e| format!("value string: {e}"))?;
        env.call_method(
            &values,
            "put",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[
                jni::objects::JValue::from(&kj),
                jni::objects::JValue::from(&vj),
            ],
        )
        .map_err(|e| format!("ContentValues.put {k}: {e}"))?;
    }
    let uri = env
        .call_method(
            resolver,
            "insert",
            "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
            &[
                jni::objects::JValue::from(&base),
                jni::objects::JValue::from(&values),
            ],
        )
        .map_err(|e| format!("resolver insert: {e}"))?
        .l()
        .map_err(|e| format!("insert uri: {e}"))?;
    if uri.is_null() {
        return Err("MediaStore insert returned null".to_string());
    }
    Ok(uri)
}

/// Writes `bytes` through `openOutputStream` (single write + loud
/// close — a failed close after a good write still fails the
/// round-trip, never a silent short file).
fn write_bytes(
    env: &mut jni::JNIEnv<'_>,
    resolver: &jni::objects::JObject<'_>,
    uri: &jni::objects::JObject<'_>,
    bytes: &[u8],
) -> Result<(), String> {
    let out = env
        .call_method(
            resolver,
            "openOutputStream",
            "(Landroid/net/Uri;)Ljava/io/OutputStream;",
            &[jni::objects::JValue::from(uri)],
        )
        .map_err(|e| format!("openOutputStream: {e}"))?
        .l()
        .map_err(|e| format!("output stream: {e}"))?;
    if out.is_null() {
        return Err("openOutputStream returned null".to_string());
    }
    let arr = env
        .byte_array_from_slice(bytes)
        .map_err(|e| format!("write buffer: {e}"))?;
    env.call_method(&out, "write", "([B)V", &[jni::objects::JValue::from(&arr)])
        .map_err(|e| format!("stream write: {e}"))?;
    env.call_method(&out, "close", "()V", &[])
        .map_err(|e| format!("stream close: {e}"))?;
    Ok(())
}

/// Reads the entry back through `openInputStream` (one reused 4
/// KiB buffer — a single local ref for the whole loop, never a
/// per-chunk leak).
fn read_bytes(
    env: &mut jni::JNIEnv<'_>,
    resolver: &jni::objects::JObject<'_>,
    uri: &jni::objects::JObject<'_>,
) -> Result<Vec<u8>, String> {
    let input = env
        .call_method(
            resolver,
            "openInputStream",
            "(Landroid/net/Uri;)Ljava/io/InputStream;",
            &[jni::objects::JValue::from(uri)],
        )
        .map_err(|e| format!("openInputStream: {e}"))?
        .l()
        .map_err(|e| format!("input stream: {e}"))?;
    if input.is_null() {
        return Err("openInputStream returned null".to_string());
    }
    let buf = env
        .new_byte_array(4096)
        .map_err(|e| format!("read buffer: {e}"))?;
    let mut out = Vec::new();
    loop {
        let n = env
            .call_method(&input, "read", "([B)I", &[jni::objects::JValue::from(&buf)])
            .map_err(|e| format!("stream read: {e}"))?
            .i()
            .map_err(|e| format!("read int: {e}"))?;
        if n <= 0 {
            break;
        }
        let mut chunk = vec![0i8; n as usize];
        env.get_byte_array_region(&buf, 0, &mut chunk)
            .map_err(|e| format!("read chunk: {e}"))?;
        out.extend(chunk.iter().map(|b| *b as u8));
    }
    env.call_method(&input, "close", "()V", &[])
        .map_err(|e| format!("stream close: {e}"))?;
    Ok(out)
}

/// Full round-trip on device: insert → write fixture → read back →
/// delete. Returns the URI spelling (record only) plus the read
/// bytes. Loud on every step.
fn mediastore_roundtrip(
    app: &AndroidApp,
    name: &str,
    bytes: &[u8],
) -> Result<(String, Vec<u8>, i32), String> {
    with_attached(app, |env, activity| {
        let resolver = resolver(env, &activity)?;
        let uri = insert_entry(env, &resolver, name)?;
        let uri_val = env
            .call_method(&uri, "toString", "()Ljava/lang/String;", &[])
            .map_err(|e| format!("uri toString: {e}"))?;
        let uri_obj = uri_val.l().map_err(|e| format!("uri string obj: {e}"))?;
        let uri_string: String = env
            .get_string(&jni::objects::JString::from(uri_obj))
            .map_err(|e| format!("uri string: {e}"))?
            .into();
        write_bytes(env, &resolver, &uri, bytes)?;
        let read = read_bytes(env, &resolver, &uri)?;
        let deleted = env
            .call_method(
                &resolver,
                "delete",
                "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",
                &[
                    jni::objects::JValue::from(&uri),
                    jni::objects::JValue::from(&jni::objects::JObject::null()),
                    jni::objects::JValue::from(&jni::objects::JObject::null()),
                ],
            )
            .map_err(|e| format!("resolver delete: {e}"))?
            .i()
            .map_err(|e| format!("delete int: {e}"))?;
        Ok((uri_string, read, deleted))
    })
}

/// Validates the picker read path on device and routes real
/// resolver bytes through the opener rendezvous: request →
/// round-trip → resolve → poll-assert, then `picker.txt` lands
/// beside the other oracle files. Returns the meta record line.
/// Loud on any mismatch (a byte drift is a platform fact, never a
/// quiet pass).
pub fn validate_picker(app: &AndroidApp, dir: &Path) -> Result<String, String> {
    use std::time::Instant;
    let t = Instant::now();
    let mut opener = AndroidFileOpener::new();
    opener.request_bytes(FilePickerOptions {
        title: "proof".to_string(),
        ..Default::default()
    });
    let Some(gen) = opener.pending_generation() else {
        return Err("picker request left nothing pending".to_string());
    };
    let (uri, read, deleted) = mediastore_roundtrip(app, FIXTURE_NAME, FIXTURE)?;
    if read.as_slice() != FIXTURE {
        return Err(format!(
            "resolver byte mismatch: fixture {} bytes, read {} bytes",
            FIXTURE.len(),
            read.len()
        ));
    }
    let files = vec![PickedFile::new(FIXTURE_NAME.to_string(), read)];
    if !opener.resolve_bytes(gen, Ok(files.clone())) {
        return Err("picker resolve discarded its own generation".to_string());
    }
    match opener.poll_bytes() {
        Some(Ok(back)) if back == files => {}
        other => {
            return Err(format!(
                "picker poll did not replay the resolved bytes: {other:?}"
            ));
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    std::fs::write(
        dir.join("picker.txt"),
        format!(
            "uri={uri}\nfixture_bytes={}\nread_bytes={}\nmatch=true\ndeleted={deleted}\n",
            FIXTURE.len(),
            files[0].bytes.len(),
        ),
    )
    .map_err(|e| format!("write picker.txt: {e}"))?;
    Ok(format!(
        "picker_bytes={} picker_match=true picker_deleted={deleted} picker_ms={ms:.0}",
        FIXTURE.len(),
    ))
}
