package com.oppa.app;

import android.app.Activity;
import android.content.Context;
import android.os.IBinder;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;

/** Soft-keyboard proxy (Round 3.1, device-pending — compiles under
 * Gradle only, never on the host).
 *
 * <p>NativeActivity owns no Java views, so no {@code InputConnection}
 * exists for the IME to talk to (that is why {@code showSoftInput}
 * on the bare decor view reports {@code false}). This class adds
 * the smallest possible view-side half: a 1x1 hidden {@code
 * EditText} whose connection forwards {@code commitText} and {@code
 * deleteSurroundingText} into Rust through the native entries the
 * bridge registers ({@code onCommitText}, {@code
 * onDeleteSurroundingText}).
 *
 * <p>Design rules (stated):
 *
 * <ul>
 *   <li>The {@code EditText} never displays text: {@code commitText}
 *       returns {@code true} (consumed) <em>without</em> calling
 *       {@code super} — Rust owns the field content, and an echo in
 *       the hidden view would prove nothing.
 *   <li>{@code setComposingText} is NOT forwarded (it returns {@code
 *       true} unconsumed-equivalent: calls {@code super} so the IME
 *       behaves, but drops the span). Live composition needs the
 *       desktop-style preedit path (session + anchor) — follow-up
 *       round, not this one.
 *   <li>All view/IMM calls hop with {@code runOnUiThread} (the
 *       {@code OppaUi} tombstone rule — off-UI-thread view calls are
 *       process-fatal). Failures log to logcat, never crash the UI
 *       thread.
 * </ul>
 */
public final class OppaIme {
    private OppaIme() {}

    private static native void onCommitText(String text);

    private static native void onDeleteSurroundingText(int before, int after);

    /** Hidden 1x1 editable view carrying the connection. */
    public static final class Bridge extends EditText {
        public Bridge(Context context) {
            super(context);
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            outAttrs.imeOptions |= EditorInfo.IME_FLAG_NO_FULLSCREEN;
            outAttrs.inputType = android.text.InputType.TYPE_CLASS_TEXT;
            return new OppaConnection(this, true);
        }

        /** The proxy: forwards the two commit primitives to Rust. */
        static final class OppaConnection extends BaseInputConnection {
            OppaConnection(View targetView, boolean fullEditor) {
                super(targetView, fullEditor);
            }

            @Override
            public boolean commitText(CharSequence text, int newCursorPosition) {
                if (text != null) {
                    onCommitText(text.toString());
                }
                return true;
            }

            @Override
            public boolean deleteSurroundingText(int beforeLength, int afterLength) {
                onDeleteSurroundingText(beforeLength, afterLength);
                return true;
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
                            bridge.setCursorVisible(false);
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
}
