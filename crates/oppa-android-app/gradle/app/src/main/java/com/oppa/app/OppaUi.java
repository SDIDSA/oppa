package com.oppa.app;

import android.app.Activity;
import android.os.Build;

/** UI-thread hop for immersive bars (v1 remainder, phone hardening).
 *
 * <p>View/Window bars calls throw {@code CalledFromWrongThreadException}
 * off the UI thread (fatal SIGABRT under CheckJNI — observed in the
 * tombstone), and NativeActivity Rust has no UI-thread hook of its
 * own. This one static, called over JNI, hops with
 * {@code runOnUiThread} and applies the whole sequence where it is
 * legal. Failures are swallowed into logcat (never a UI-thread
 * crash); success is verified by the window size afterward, never
 * assumed.
 */
public final class OppaUi {
    private OppaUi() {}

    public static void hideBars(final Activity activity) {
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            android.view.Window window = activity.getWindow();
                            android.view.View view = window.getDecorView();
                            view.setSystemUiVisibility(
                                    0x1706); // IMMERSIVE_STICKY | HIDE_NAVIGATION
                            // | FULLSCREEN | LAYOUT_HIDE_NAVIGATION
                            // | LAYOUT_FULLSCREEN | LAYOUT_STABLE
                            window.addFlags(0x200); // FLAG_LAYOUT_NO_LIMITS
                            android.view.WindowManager.LayoutParams params =
                                    window.getAttributes();
                            // Draw into the display cutout strip
                            // (decision 396): the default cutout mode
                            // letterboxes the window below the status
                            // strip to dodge the camera, so the
                            // containing frame stays 2290 tall even
                            // with the bars hidden and an explicit
                            // 1080x2400 ask. A fullscreen immersive
                            // scene owns the strip (short edges only —
                            // the camera dot stays clear of the
                            // top-left badge). Field exists since API
                            // 28, minSdk is 29 — no version guard.
                            params.layoutInDisplayCutoutMode =
                                    android.view.WindowManager.LayoutParams
                                            .LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                            // Force full-display size with EXPLICIT pixels:
                            // hiding the bars does not re-lay-out an
                            // already-created native window (observed
                            // 1080x2290 stuck), and MATCH_PARENT equals
                            // the current attrs (no relayout); explicit
                            // differing values force it.
                            android.util.DisplayMetrics metrics =
                                    new android.util.DisplayMetrics();
                            activity.getWindowManager()
                                    .getDefaultDisplay()
                                    .getRealMetrics(metrics);
                            params.width = metrics.widthPixels;
                            params.height = metrics.heightPixels;
                            params.flags |= 0x200;
                            window.setAttributes(params);
                            if (Build.VERSION.SDK_INT >= 30) {
                                window.setDecorFitsSystemWindows(false);
                                android.view.WindowInsetsController controller =
                                        window.getInsetsController();
                                if (controller != null) {
                                    controller.hide(
                                            android.view.WindowInsets.Type.navigationBars()
                                                    | android.view.WindowInsets.Type
                                                            .statusBars());
                                    controller.setSystemBarsBehavior(
                                            android.view.WindowInsetsController
                                                    .BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                                }
                            }
                        } catch (Throwable t) {
                            android.util.Log.e("OppaUi", "hideBars failed", t);
                        }
                    }
                });
    }
}
