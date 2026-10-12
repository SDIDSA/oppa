package com.oppa.app;

import android.app.Activity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;

/** SAF open trampoline (decision 427 — the result-delivery gap).
 *
 * <p>NativeActivity cannot receive {@code onActivityResult}
 * ({@code android-activity} 0.6 forwards no result events), so the
 * picker needs a Java receiver: this transparent activity fires
 * {@code ACTION_OPEN_DOCUMENT} in {@code onCreate} and forwards
 * the outcome to Rust through {@code onPickerResult}, then
 * finishes. No UI of its own (translucent theme, excluded from
 * recents, no history).
 *
 * <p>Result contract (mirrors the Rust rendezvous): content bytes
 * + display name + echoed generation + dismissed flag on pick;
 * dismissal settles empty, read failure settles loud (never a
 * partial read, never half state). Bytes are
 * read here (not over JNI streams) and cached app-private by Rust,
 * so the framework's {@code PathBuf} contract stays honest —
 * never a fabricated filesystem path.
 */
public final class OppaPickerActivity extends Activity {
    private static final int REQ_OPEN = 7;

    static {
        // Associates the Rust cdylib with the app loader, so the
        // static `Java_…onPickerResult` export below resolves when
        // the system dispatches through this class (a `RegisterNatives`
        // on a DexClassLoader-loaded copy misses — different class
        // object). Runs at first active use, which is always through
        // the app loader (system instantiation or explicit load);
        // never force initialization from native (that would run
        // `<clinit>` under the wrong loader and fail loudly).
        System.loadLibrary("oppa_android_app");
    }

    private static native void onPickerResult(
            byte[] content, String name, int generation, boolean dismissed);

    /** Fires the picker from any thread (UI-thread hop — the
     * {@code OppaUi} tombstone rule: activity calls off the UI
     * thread are process-fatal). Failures log to logcat; Rust
     * sees them as a stuck-pending request, never a crash. */
    public static void pick(
            final Activity caller,
            final String[] mimeTypes,
            final boolean multiple,
            final int generation) {
        caller.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            Intent trampoline = new Intent(caller, OppaPickerActivity.class);
                            trampoline.putExtra("mimeTypes", mimeTypes);
                            trampoline.putExtra("multiple", multiple);
                            trampoline.putExtra("generation", generation);
                            caller.startActivity(trampoline);
                        } catch (Throwable t) {
                            android.util.Log.e("OppaPicker", "pick failed", t);
                        }
                    }
                });
    }

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent src = getIntent();
        String[] mimeTypes =
                src != null ? src.getStringArrayExtra("mimeTypes") : null;
        boolean multiple = src != null && src.getBooleanExtra("multiple", false);
        int generation = src != null ? src.getIntExtra("generation", 0) : 0;
        Intent open = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        open.addCategory(Intent.CATEGORY_OPENABLE);
        open.setType("*/*");
        if (mimeTypes != null && mimeTypes.length > 0) {
            open.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
        }
        open.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
        try {
            startActivityForResult(open, REQ_OPEN);
        } catch (Throwable t) {
            android.util.Log.e("OppaPicker", "open intent failed", t);
            onPickerResult(null, null, generation, false);
            finish();
            return;
        }
        // Store the generation across the chooser round-trip.
        getIntent().putExtra("generation", generation);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        int generation = getIntent().getIntExtra("generation", 0);
        if (requestCode == REQ_OPEN
                && resultCode == RESULT_OK
                && data != null
                && data.getData() != null) {
            Uri uri = data.getData();
            String name = displayName(uri);
            byte[] content = readAll(uri);
            // Null content with a live URI is a read failure (loud
            // on the Rust side); dismissal carries dismissed=true.
            onPickerResult(content, name, generation, content == null);
        } else {
            onPickerResult(null, null, generation, true);
        }
        finish();
    }

    /** Display name via {@code OpenableColumns} (fallback
     * {@code "picked"} — a null name never reaches Rust). */
    private String displayName(Uri uri) {
        Cursor cursor = null;
        try {
            cursor =
                    getContentResolver()
                            .query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null);
            if (cursor != null && cursor.moveToFirst()) {
                String name = cursor.getString(0);
                if (name != null && !name.isEmpty()) {
                    return name;
                }
            }
        } catch (Throwable t) {
            android.util.Log.e("OppaPicker", "display name failed", t);
        } finally {
            if (cursor != null) {
                cursor.close();
            }
        }
        return "picked";
    }

    /** Fully reads the URI (null on any failure — Rust settles the
     * loud failure from the null, never a partial read). */
    private byte[] readAll(Uri uri) {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[4096];
        try {
            InputStream input = getContentResolver().openInputStream(uri);
            if (input == null) {
                return null;
            }
            try {
                int n;
                while ((n = input.read(buf)) > 0) {
                    out.write(buf, 0, n);
                }
            } finally {
                try {
                    input.close();
                } catch (Throwable ignored) {
                }
            }
        } catch (Throwable t) {
            android.util.Log.e("OppaPicker", "read failed", t);
            return null;
        }
        return out.toByteArray();
    }
}
