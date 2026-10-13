// Template for per-app shells (`cargo oppa` substitutes
// __PACKAGE__ + __LIB__ — never compiles with the tokens in place).
package __PACKAGE__;

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

    // No `System.loadLibrary` here on purpose: the activity's own
    // loader already opened the `.so`, and a second namespace
    // loading it again dies with `UnsatisfiedLinkError` inside
    // `<clinit>` before any method body runs (observed on-device).
    // Native entries bind by explicit `RegisterNatives` from Rust
    // instead — no name lookup, no loader association needed.

    private static native void onCommitText(String text);

    private static native void onDeleteSurroundingText(int before, int after);

    private static native void onComposeText(String text);

    // Finish-composing report (decision 461): `finishComposingText`
    // asks the editor to commit the open composing buffer — without
    // forwarding, a bare commit arriving right after (the Gboard
    // space flow) replaces the word with the suffix alone.
    private static native void onFinishComposing();

    // Proxy selection report (decision 449, retained past the menu
    // retirement in decision 456): native selection moves the proxy
    // buffer itself makes (software-keyboard arrows, Editor word
    // selects) still cross to Rust so the session follows the
    // visible span. Named distinct from the
    // View.onSelectionChanged override below (same arity would
    // recurse into itself instead of crossing JNI).
    private static native void onProxySelectionChanged(int start, int end);

    /** Hidden 1x1 editable view carrying the connection. */
    public static final class Bridge extends EditText {
        public Bridge(Context context) {
            super(context);
        }

        // No selection UI lives here anymore (decision 456
        // retires the system toolbar — the scene overlay in
        // `host.toolbar_model` is the only toolbar): the proxy owns
        // no ActionMode, starts none, and hosts no handles. Taps
        // resolve geometrically in the Rust router against scene
        // pixels; the Editor never enters selection mode on this
        // view. What stays: the hidden buffer mirroring below (IME
        // coherence) and the selection-change report (software
        // keyboards move the proxy buffer themselves).

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
        // fields — plain fields keep clipboard and learning). Static
        // like mirrorSync (one proxy per window generation).
        private static volatile boolean passwordMode = false;

        /** Re-mirrors the live session text/selection (decision 456:
         * the only mirror left — text/selection follow the session
         * for IME coherence; no toolbar fires, none stands, none is
         * finished here).
         */
        boolean mirrorUpdate(String text, int start, int end, Rect rect) {
            mirror(text, start, end, rect);
            return true;
        }

        /** Mirrors one Rust frame (decision 440 text/IME coherence —
         * the hidden buffer must track the session for the IME's
         * context queries). CHANGE-GATED (decision 450): setText
         * rebuilds the buffer and drops all spans — firing it while
         * the Editor holds live state (a software-keyboard gesture
         * in flight) yanks it out from under the finger. Text and
         * selection apply only when they differ from the live proxy,
         * so a mirror mid-gesture is a complete no-op and the gesture
         * owns its state until release.
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

        /** Wears the mirrored field box: the floating toolbar clips
         * the content rect to the originating view's visible bounds,
         * so the proxy overlays the field it mirrors (transparent
         * text/background, touches pass through: it anchors, never
         * renders, never interacts). Parent is the decor content
         * FrameLayout (`addContentView`), so margins place it; screen
         * px == parent px under the full-bleed window. Skips redundant
         * layouts (same rect twice never re-lays-out). Positions
         * SYNCHRONOUSLY (measure + layout now, not just params):
         * setLayoutParams only requests a later traversal, but the
         * mode queries our screen origin in the same UI tick — a
         * still-1x1 proxy at (0,0) would again clip the rect to
         * empty (the proven silent failure). Both are kept: params
         * govern future traversals (rotation, keyboard geometry),
         * direct layout governs this tick.
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
            // Pass-through (decision 447, trigger retired in 456):
            // the proxy owns no interaction — taps must reach the
            // NativeActivity queue (the framework router), never the
            // proxy's caret/selection machinery. Returning false
            // bubbles to the window.
            return false;
        }

        @Override
        public boolean onKeyDown(int keyCode, android.view.KeyEvent event) {
            // Hardware-key trace (decision 459 — dispositions log so
            // a dead physical keyboard diagnoses: commits log through
            // onCommitText below, fall-through returns false into the
            // activity native queue, super handles the rest).
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
            // per-field input variation instead (below): plain fields
            // stay learnable and clipboard-capable, masked fields keep
            // incognito exactly where it belongs. Latency-neutral: 440
            // measured typing speed immune to the flag either way.
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
            // Proxy selection report (decision 449, narrowed in 456):
            // selection moves the proxy buffer itself makes
            // (software-keyboard arrows, Editor word selects) cross
            // to Rust so the session follows the visible span.
            // Skipped while a mirror is in flight (echo of our own
            // setSelection) and for empty selections (no span to
            // follow — collapsed carets are Rust-owned); the pump
            // converts units to bytes.
            if (Bridge.mirrorSync || selStart < 0 || selEnd < 0) {
                return;
            }
            onProxySelectionChanged(selStart, selEnd);
        }

        /** The proxy: forwards the commit primitives plus live
         * composition to Rust. */
        static final class OppaConnection extends BaseInputConnection {            OppaConnection(View targetView, boolean fullEditor) {
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
                // Guarded (decision 449): the echo moves the proxy
                // selection, which must not report back as a gesture.
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
                // Rust still owns the pixels. Guarded (decision 449):
                // the echo moves the proxy selection, which must not
                // report back as a gesture.
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
                // Guarded (decision 449): like commitText above, the
                // echo must not report back as a gesture.
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
                // Guarded (decision 449): like the commit/delete echo
                // above, the proxy-buffer finish must not report back
                // as a gesture.
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
                            // Long-clickable OFF (decision 456
                            // reverses decision 447's ON — the only
                            // reason for arming was the synthetic
                            // long-press trigger): real touches never
                            // reach super (onTouchEvent returns false
                            // first), so this changes nothing for
                            // them. No setTextIsSelectable call
                            // (decision 445 added one to suppress
                            // handles — it also converts the buffer
                            // to NORMAL and drops the movement
                            // method, silently killing proxy
                            // selection and every gate that reads
                            // it; the software keyboard still needs
                            // a live editable buffer).
                            bridge.setLongClickable(false);
                            bridge.setCursorVisible(false);
                            // The proxy anchors but never renders: null
                            // background + transparent ink keep the
                            // overlaid field visually ours.
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
     * IME re-reads the connection (a shared proxy keeps focus across
     * field switches, so a bare flag would only take on the next
     * fresh connection). Rust calls this before every keyboard show
     * with the focused session's masked state; posts in order from
     * the same thread, so the show below always sees the fresh flag.
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
     * The view must be the attached proxy (Rust passes it — attach
     * happens lazily there). Synchronous with a UI-thread gate (the
     * `attach` precedent): returns whether the mirror ran, so Rust
     * logs loudly instead of believing a refused write. Never call
     * from the UI thread itself (the gate would sleep the full
     * timeout — all callers are the native loop thread).
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

    /** Waits for a UI-thread gate (the `attach` precedent — bounded,
     * interrupt-preserving). A timeout returns with the work flag as
     * the Runnable left it (false when it never ran): the caller
     * retries next pump, never wedges.
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
