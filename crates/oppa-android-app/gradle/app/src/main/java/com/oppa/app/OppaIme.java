package com.oppa.app;

import android.app.Activity;
import android.content.Context;
import android.graphics.Rect;
import android.os.IBinder;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;

/** Soft-keyboard proxy (Round 3.1, decision 438 extends it with
 * live composition — compiles under Gradle only, never on the host).
 *
 * <p>NativeActivity owns no Java views, so no {@code InputConnection}
 * exists for the IME to talk to (that is why {@code showSoftInput}
 * on the bare decor view reports {@code false}). This class adds
 * the smallest possible view-side half: a 1x1 hidden {@code
 * EditText} whose connection forwards {@code commitText}, {@code
 * deleteSurroundingText}, and {@code setComposingText} into Rust
 * through the native entries the bridge registers ({@code
 * onCommitText}, {@code onDeleteSurroundingText}, {@code
 * onComposeText}).
 *
 * <p>Design rules (stated):
 *
 * <ul>
 *   <li>The {@code EditText} never displays text: {@code commitText}
 *       returns {@code true} (consumed) <em>without</em> calling
 *       {@code super} — Rust owns the field content, and an echo in
 *       the hidden view would prove nothing.
 *   <li>{@code setComposingText} forwards the live composing string
 *       to Rust (the session renders the preedit, desktop parity)
 *       <em>and</em> calls {@code super}, so the hidden buffer stays
 *       coherent for the IME's context queries. The caret rides at
 *       the end of the text (Android cursor units don't map onto
 *       byte carets — stated simplification).
 *   <li>All view/IMM calls hop with {@code runOnUiThread} (the
 *       {@code OppaUi} tombstone rule — off-UI-thread view calls are
 *       process-fatal). Failures log to logcat, never crash the UI
 *       thread.
 * </ul>
 */
public final class OppaIme {
    private OppaIme() {}

    static {
        // Same loader rule as `OppaPickerActivity` (dormant until
        // the Bridge view goes live — landing the association now,
        // not at first keystroke; never force-init from native).
        System.loadLibrary("oppa_android_app");
    }

    private static native void onCommitText(String text);

    private static native void onDeleteSurroundingText(int before, int after);

    private static native void onComposeText(String text);

    // Finish-composing report (decision 461, mirrors the template).
    private static native void onFinishComposing();

    // Proof-copy note: this file mirrors the runner template's
    // `OppaIme.java` for the shared proxy behavior (template owns
    // the shape; this copy adds only the proof loader block above).
    // Proxy selection report (decision 449, retained past the menu
    // retirement in decision 456): native selection moves the proxy
    // buffer itself makes still cross to Rust so the session follows
    // the visible span.
    private static native void onProxySelectionChanged(int start, int end);

    /** Hidden 1x1 editable view carrying the connection. */
    public static final class Bridge extends EditText {
        public Bridge(Context context) {
            super(context);
        }

        // No selection UI lives here anymore (decision 456
        // retires the system toolbar — mirrors the template: the
        // scene overlay is the only toolbar, the proxy owns no
        // ActionMode and starts none).
        // Proof-copy note: this block mirrors the template
        // (package aside).

        // True while a mirror write is in flight (UI thread sets,
        // IME thread reads — volatile, never synchronized): the
        // connection forwards below skip their native callbacks then,
        // so programmatic syncs never echo back into the Rust session
        // as user commits. Scoped try/finally at every set site, so a
        // stuck flag (which would swallow real typing) is
        // unrepresentable.
        private static volatile boolean mirrorSync = false;

        // Password variation for the next connection (decision 444):
        // Rust publishes the focused field's masked state on every
        // keyboard show; the connection declares the password
        // variation only then (Gboard incognito scoped to password
        // fields). Mirrors the runner template's `OppaIme.java`.
        private static volatile boolean passwordMode = false;

        /** Re-mirrors the live session text/selection (decision 456,
         * mirrors the template — the only mirror left, no toolbar
         * fires, none stands).
         */
        boolean mirrorUpdate(String text, int start, int end, Rect rect) {
            mirror(text, start, end, rect);
            return true;
        }

        /** Mirrors one Rust frame (decision 440 text/IME coherence —
         * mirrors the template: buffer tracks the session for the
         * IME's queries). CHANGE-GATED (decision 450, mirrors the
         * template): text and selection apply only when they differ,
         * so mirrors never disturb live Editor state.
         */
        private String lastMirrorText = "";

        private void mirror(String text, int start, int end, Rect rect) {
            mirrorSync = true;
            try {
                if (text != null && !text.equals(lastMirrorText)) {
                    setText(text);
                    lastMirrorText = text;
                }
                int len = length();
                int s = Math.max(0, Math.min(start, len));
                int e = Math.max(0, Math.min(end, len));
                if (getSelectionStart() != s || getSelectionEnd() != e) {
                    setSelection(s, e);
                }
                overlayField(rect);
            } finally {
                mirrorSync = false;
            }
        }

        private final Rect lastOverlay = new Rect();

        /** Wears the mirrored field box (mirrors the runner template:
         * the floating toolbar clips the content rect to the
         * originating view's visible bounds — positions synchronously
         * because the mode queries our origin in the same UI tick).
         */
        private void overlayField(Rect rect) {
            if (rect.equals(lastOverlay)) {
                return;
            }
            int w = Math.max(1, rect.right - rect.left);
            int h = Math.max(1, rect.bottom - rect.top);
            android.view.ViewGroup.LayoutParams lp = getLayoutParams();
            if (lp instanceof android.widget.FrameLayout.LayoutParams) {
                android.widget.FrameLayout.LayoutParams flp =
                        (android.widget.FrameLayout.LayoutParams) lp;
                flp.width = w;
                flp.height = h;
                flp.leftMargin = rect.left;
                flp.topMargin = rect.top;
                flp.gravity = android.view.Gravity.TOP | android.view.Gravity.LEFT;
                setLayoutParams(flp);
            }
            measure(
                    android.view.View.MeasureSpec.makeMeasureSpec(
                            w, android.view.View.MeasureSpec.EXACTLY),
                    android.view.View.MeasureSpec.makeMeasureSpec(
                            h, android.view.View.MeasureSpec.EXACTLY));
            layout(rect.left, rect.top, rect.right, rect.bottom);
            lastOverlay.set(rect);
        }

        @Override
        public boolean onTouchEvent(android.view.MotionEvent event) {
            // Pass-through (mirrors the template — the proxy owns no
            // interaction, taps bubble to the window).
            return false;
        }

        @Override
        public boolean onKeyDown(int keyCode, android.view.KeyEvent event) {
            // Hardware-key trace (decision 459, mirrors the template —
            // dispositions log so a dead physical keyboard diagnoses).
            // Chord fall-through (decision 442): Ctrl combinations
            // and Shift+navigation travel to the activity and the
            // native queue, where the runner's shortcut/nav matrix
            // owns them (select-all, undo/redo, selection extension,
            // word jumps). Consuming them here would strand the
            // session (hidden caret moves invisibly, shortcuts die).
            // Excluded: Alt/Meta chords (AltGr typing, system keys),
            // Ctrl+Space (IME switcher), Ctrl+Del (keeps today's
            // single-char delete through deleteSurroundingText).
            if (event != null && !event.isAltPressed() && !event.isMetaPressed()) {
                if (event.isCtrlPressed()
                        && keyCode != android.view.KeyEvent.KEYCODE_SPACE
                        && keyCode != android.view.KeyEvent.KEYCODE_DEL
                        && keyCode != android.view.KeyEvent.KEYCODE_FORWARD_DEL) {
                    return false;
                }
                if (event.isShiftPressed()
                        && (keyCode == android.view.KeyEvent.KEYCODE_DPAD_UP
                                || keyCode == android.view.KeyEvent.KEYCODE_DPAD_DOWN
                                || keyCode == android.view.KeyEvent.KEYCODE_DPAD_LEFT
                                || keyCode == android.view.KeyEvent.KEYCODE_DPAD_RIGHT
                                || keyCode == android.view.KeyEvent.KEYCODE_MOVE_HOME
                                || keyCode == android.view.KeyEvent.KEYCODE_MOVE_END)) {
                    return false;
                }
            }
            // Printable hardware keys become single-char commits
            // (decision 438): a focused editable consumes them into
            // its own buffer, so without this they never reach the
            // native queue past the proxy (adb/hardware typing dies
            // visibly silent). The Unicode comes from the platform
            // map (shift/symbols handled there; dead-key combining
            // stays simple ASCII); Ctrl chords fall through (else
            // they would land as control chars in the field).
            // Deletions keep default handling (they arrive as
            // deleteSurroundingText, proven); navigation/action keys
            // return false so they travel to the activity and the
            // native queue (the runner Key arm owns them — consuming
            // them here would strand the session caret).
            if (event != null
                    && !event.isCtrlPressed()
                    && keyCode != android.view.KeyEvent.KEYCODE_DEL
                    && keyCode != android.view.KeyEvent.KEYCODE_FORWARD_DEL
                    && keyCode != android.view.KeyEvent.KEYCODE_DPAD_UP
                    && keyCode != android.view.KeyEvent.KEYCODE_DPAD_DOWN
                    && keyCode != android.view.KeyEvent.KEYCODE_DPAD_LEFT
                    && keyCode != android.view.KeyEvent.KEYCODE_DPAD_RIGHT
                    && keyCode != android.view.KeyEvent.KEYCODE_DPAD_CENTER
                    && keyCode != android.view.KeyEvent.KEYCODE_ENTER
                    && keyCode != android.view.KeyEvent.KEYCODE_NUMPAD_ENTER
                    && keyCode != android.view.KeyEvent.KEYCODE_TAB
                    && keyCode != android.view.KeyEvent.KEYCODE_ESCAPE) {
                int c = event.getUnicodeChar(event.getMetaState());
                if (c != 0) {
                    onCommitText(String.valueOf((char) c));
                    return true;
                }
                android.util.Log.i("OppaIme", "onKeyDown no unicode for " + keyCode);
            }
            if (keyCode == android.view.KeyEvent.KEYCODE_DPAD_UP
                    || keyCode == android.view.KeyEvent.KEYCODE_DPAD_DOWN
                    || keyCode == android.view.KeyEvent.KEYCODE_DPAD_LEFT
                    || keyCode == android.view.KeyEvent.KEYCODE_DPAD_RIGHT
                    || keyCode == android.view.KeyEvent.KEYCODE_DPAD_CENTER
                    || keyCode == android.view.KeyEvent.KEYCODE_MOVE_HOME
                    || keyCode == android.view.KeyEvent.KEYCODE_MOVE_END
                    || keyCode == android.view.KeyEvent.KEYCODE_ENTER
                    || keyCode == android.view.KeyEvent.KEYCODE_NUMPAD_ENTER
                    || keyCode == android.view.KeyEvent.KEYCODE_TAB
                    || keyCode == android.view.KeyEvent.KEYCODE_ESCAPE) {
                android.util.Log.i("OppaIme", "onKeyDown fall-through " + keyCode);
                return false;
            }
            return super.onKeyDown(keyCode, event);
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            outAttrs.imeOptions |= EditorInfo.IME_FLAG_NO_FULLSCREEN;
            // No IME_FLAG_NO_PERSONALIZED_LEARNING here (decision 444
            // reverses decision 440's hygiene flag): Gboard maps that
            // flag to full incognito mode on EVERY field, which
            // disables clipboard history/suggestions globally — the
            // reported missing-clipboard cause. Password privacy rides
            // per-field input variation instead (below). Mirrors the
            // runner template's `OppaIme.java` verbatim.
            if (passwordMode) {
                outAttrs.inputType =
                        android.text.InputType.TYPE_CLASS_TEXT
                                | android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD;
            } else {
                outAttrs.inputType = android.text.InputType.TYPE_CLASS_TEXT;
            }
            android.util.Log.i("OppaIme", "onCreateInputConnection: proxy connection created");
            return new OppaConnection(this, true);
        }

        @Override
        protected void onSelectionChanged(int selStart, int selEnd) {
            super.onSelectionChanged(selStart, selEnd);
            // Proxy gesture report (decision 449, mirrors the
            // template — skipped while mirroring and when empty).
            if (Bridge.mirrorSync || selStart < 0 || selEnd < 0) {
                return;
            }
            onProxySelectionChanged(selStart, selEnd);
        }

        /** The proxy: forwards the commit primitives plus live
         * composition to Rust. */
        static final class OppaConnection extends BaseInputConnection {
            OppaConnection(View targetView, boolean fullEditor) {
                super(targetView, fullEditor);
            }

            @Override
            public boolean setComposingText(CharSequence text, int newCursorPosition) {
                android.util.Log.i(
                        "OppaIme",
                        "setComposingText len="
                                + (text == null ? -1 : text.length())
                                + " cursor="
                                + newCursorPosition);
                if (text != null && !Bridge.mirrorSync) {
                    onComposeText(text.toString());
                    android.util.Log.i("OppaIme", "setComposingText forwarded to native");
                }
                // Guarded (decision 449, mirrors the template).
                Bridge.mirrorSync = true;
                try {
                    return super.setComposingText(text, newCursorPosition);
                } finally {
                    Bridge.mirrorSync = false;
                }
            }

            @Override
            public boolean commitText(CharSequence text, int newCursorPosition) {
                android.util.Log.i(
                        "OppaIme",
                        "commitText len=" + (text == null ? -1 : text.length()));
                if (text != null && !Bridge.mirrorSync) {
                    onCommitText(text.toString());
                }
                // Sync the hidden buffer too (decision 440): the IME
                // tracks editor state through context queries and
                // selection callbacks — without the echo its view of
                // the field drifts from the session and every key pays
                // a slow recovery path. The view stays 1x1 invisible;
                // Rust still owns the pixels. Guarded (decision 449,
                // mirrors the template).
                Bridge.mirrorSync = true;
                try {
                    super.commitText(text, newCursorPosition);
                } finally {
                    Bridge.mirrorSync = false;
                }
                return true;
            }

            @Override
            public boolean deleteSurroundingText(int beforeLength, int afterLength) {
                android.util.Log.i(
                        "OppaIme",
                        "deleteSurroundingText " + beforeLength + "," + afterLength);
                if (!Bridge.mirrorSync) {
                    onDeleteSurroundingText(beforeLength, afterLength);
                }
                // Guarded (decision 449, mirrors the template).
                Bridge.mirrorSync = true;
                try {
                    super.deleteSurroundingText(beforeLength, afterLength);
                } finally {
                    Bridge.mirrorSync = false;
                }
                return true;
            }

            @Override
            public boolean finishComposingText() {
                android.util.Log.i("OppaIme", "finishComposingText");
                if (!Bridge.mirrorSync) {
                    onFinishComposing();
                }
                // Guarded (decision 449, mirrors the template).
                Bridge.mirrorSync = true;
                try {
                    return super.finishComposingText();
                } finally {
                    Bridge.mirrorSync = false;
                }
            }
        }
    }

    /** Adds the hidden view to the window (UI thread). Returns the
     * view for show/focus calls, or null on failure (loud logcat).
     */
    public static View attach(final Activity activity) {
        final View[] holder = new View[1];
        final Throwable[] failure = new Throwable[1];
        final Object gate = new Object();
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            Bridge bridge = new Bridge(activity);
                            bridge.setFocusable(true);
                            bridge.setFocusableInTouchMode(true);
                            bridge.setClickable(false);
                            // Long-clickable OFF (decision 456, mirrors
                            // the template — the synthetic trigger is
                            // gone; real touches bypass super anyway).
                            // No setTextIsSelectable (it converts the
                            // buffer off Spannable and drops movement,
                            // silently killing proxy selection).
                            bridge.setLongClickable(false);
                            bridge.setCursorVisible(false);
                            // Anchor-only proxy (mirrors the runner
                            // template).
                            bridge.setBackground(null);
                            bridge.setTextColor(android.graphics.Color.TRANSPARENT);
                            android.view.WindowManager.LayoutParams params =
                                    new android.view.WindowManager.LayoutParams(
                                            1,
                                            1,
                                            android.view.WindowManager.LayoutParams.TYPE_APPLICATION,
                                            0,
                                            android.graphics.PixelFormat.TRANSPARENT);
                            activity.addContentView(bridge, params);
                            holder[0] = bridge;
                        } catch (Throwable t) {
                            failure[0] = t;
                        } finally {
                            synchronized (gate) {
                                gate.notify();
                            }
                        }
                    }
                });
        synchronized (gate) {
            try {
                gate.wait(5000);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
        if (failure[0] != null) {
            android.util.Log.e("OppaIme", "attach failed", failure[0]);
            return null;
        }
        return holder[0];
    }

    /** Focuses the proxy view and shows the keyboard (UI thread). */
    public static void showIme(final Activity activity, final View view) {
        android.util.Log.i("OppaIme", "showIme: focusing proxy, showing keyboard");
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            view.requestFocus();
                            InputMethodManager imm =
                                    (InputMethodManager)
                                            activity.getSystemService(
                                                    Context.INPUT_METHOD_SERVICE);
                            if (imm != null) {
                                imm.showSoftInput(view, 0);
                            }
                        } catch (Throwable t) {
                            android.util.Log.e("OppaIme", "showIme failed", t);
                        }
                    }
                });
    }

    /** Publishes the focused field's password mode (decision 444):
     * stores the masked flag and pokes {@code restartInput} so the
     * IME re-reads the connection. Mirrors the runner template's
     * `OppaIme.java` verbatim (package aside).
     */
    public static void applyPasswordMode(
            final Activity activity, final View view, final boolean password) {
        if (!(view instanceof Bridge)) {
            android.util.Log.e("OppaIme", "applyPasswordMode: view is not the proxy");
            return;
        }
        final Bridge bridge = (Bridge) view;
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            bridge.passwordMode = password;
                            InputMethodManager imm =
                                    (InputMethodManager)
                                            activity.getSystemService(
                                                    Context.INPUT_METHOD_SERVICE);
                            if (imm != null) {
                                imm.restartInput(bridge);
                            }
                        } catch (Throwable t) {
                            android.util.Log.e("OppaIme", "applyPasswordMode failed", t);
                        }
                    }
                });
    }

    /** Hides the keyboard from the window token (UI thread). */
    public static void hideIme(final Activity activity, final IBinder token) {
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            InputMethodManager imm =
                                    (InputMethodManager)
                                            activity.getSystemService(
                                                    Context.INPUT_METHOD_SERVICE);
                            if (imm != null) {
                                imm.hideSoftInputFromWindow(token, 0);
                            }
                        } catch (Throwable t) {
                            android.util.Log.e("OppaIme", "hideIme failed", t);
                        }
                    }
                });
    }

    /** Mirrors one Rust selection frame into the proxy buffer
     * (decision 456 — text + UTF-16 selection mirror in one
     * UI-thread hop for IME coherence; no toolbar fires anymore).
     * Mirrors the runner template (package aside). Never call from
     * the UI thread itself.
     */
    public static boolean updateMenu(
            final Activity activity,
            final View view,
            final String text,
            final int start,
            final int end,
            final int l,
            final int t,
            final int r,
            final int b) {
        if (!(view instanceof Bridge)) {
            android.util.Log.e("OppaIme", "updateMenu: view is not the proxy");
            return false;
        }
        final Bridge bridge = (Bridge) view;
        final Rect rect = new Rect(l, t, r, b);
        final boolean[] out = new boolean[1];
        final Object gate = new Object();
        // runOnUiThread (not view.post): post drops the Runnable
        // silently when the view is momentarily detached (heal
        // edges) — the activity hop always runs or times out, never
        // vanishes. The view-side work is identical either way.
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            boolean live = bridge.mirrorUpdate(text, start, end, rect);
                            android.util.Log.i(
                                    "OppaIme", "updateMenu ok=" + live + " rect=" + rect);
                            out[0] = live;
                        } catch (Throwable t) {
                            android.util.Log.e("OppaIme", "updateMenu failed", t);
                        } finally {
                            synchronized (gate) {
                                gate.notify();
                            }
                        }
                    }
                });
        gateWait(gate, "updateMenu");
        return out[0];
    }

    /** Waits for a UI-thread gate (bounded, interrupt-preserving).
     * A timeout returns with the work flag as the Runnable left it
     * (false when it never ran): the caller retries next pump, never
     * wedges.
     */
    private static void gateWait(Object gate, String name) {
        synchronized (gate) {
            try {
                gate.wait(2000);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                android.util.Log.e("OppaIme", name + ": gate interrupted", e);
            }
        }
    }
}
