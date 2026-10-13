// Template for per-app shells (`cargo oppa` substitutes
// com.oppa.login — never compiles with the tokens in place).
package com.oppa.login;

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
 *
 * <p>Both halves follow the platform recipes (minSdk 29): the legacy
 * {@code SYSTEM_UI_FLAG_*} path for API 29 plus the
 * {@link android.view.WindowInsetsController} path on API 30+
 * ({@code setDecorFitsSystemWindows(false)} + {@code hide}/{@code show}
 * over {@code statusBars() | navigationBars()}), kept side by side —
 * the 30+ compat mapping still honors the legacy bits, so the two
 * halves agree instead of fighting.
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
                            android.util.Log.i(
                                    "OppaUi",
                                    "hideBars pre: decor=0x"
                                            + Integer.toHexString(view.getSystemUiVisibility())
                                            + " window=0x"
                                            + Integer.toHexString(
                                                    view.getWindowSystemUiVisibility()));
                            // Legacy hide bits (the API 29 path — honored
                            // on 30+ through the compat mapping, so kept
                            // unconditionally): HIDE_NAVIGATION +
                            // FULLSCREEN hide both bars while the LAYOUT
                            // trio keeps the layout stable across the
                            // flip. IMMERSIVE_STICKY is the documented
                            // persistent-hide modifier: without it the
                            // framework force-clears HIDE_NAVIGATION on
                            // any touch (and FULLSCREEN on a top-edge
                            // swipe), so hidden mode would end on API 29
                            // at the first tap. Sticky answers a swipe
                            // with a transient auto-hiding peek — the
                            // exact semantics of
                            // BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                            // below, so both API paths agree. The show
                            // half unwinds every bit via
                            // setSystemUiVisibility(0); the historic
                            // stuck-status was the FLAG_FULLSCREEN
                            // *window* flag (the compat path keeps
                            // statusBars hidden while it stands on a
                            // match-parent window, sysUi bits or not),
                            // which showBars now clears — never the
                            // immersive bit.
                            view.setSystemUiVisibility(
                                    android.view.View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                                            | android.view.View.SYSTEM_UI_FLAG_FULLSCREEN
                                            | android.view.View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                                            | android.view.View
                                                    .SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                                            | android.view.View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                                            | android.view.View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);
                            // Pre-30 only: arm the window fullscreen flag
                            // alongside the sysUi bits above, so both
                            // hide channels agree. SysUi-only hide leaves
                            // an already-drawn status bar undrawn on API
                            // 29 (proven: nav cycles, drawn status never
                            // undraws); the plain `NoTitleBar` theme sets
                            // no flag by itself, hence this explicit arm.
                            // Untouched on 30+ (proven clean without it).
                            if (Build.VERSION.SDK_INT < 30) {
                                window.addFlags(
                                        android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN);
                            }
                            // Full-bleed pair: the window extends into
                            // the bar regions instead of being inset by
                            // them (the scene ignores insets, so shown
                            // bars later overlay it — never a resize).
                            window.addFlags(
                                    android.view.WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                                            | android.view.WindowManager.LayoutParams
                                                    .FLAG_LAYOUT_NO_LIMITS);
                            if (Build.VERSION.SDK_INT < 30) {
                                // Pre-30 only: drop the bar-background
                                // ownership showBars takes below. While
                                // DRAWS_SYSTEM_BAR_BACKGROUNDS stands,
                                // the pre-30 policy keeps the STATUS_BAR
                                // window alive across the re-hide (nav
                                // hides, status sticks drawn — proven on
                                // API 29) even with the fullscreen flag
                                // and hide bits set; without it the hide
                                // state matches boot exactly and both
                                // bars go. Show re-adds it every time.
                                // The 30+ path is untouched (proven with
                                // the flag standing).
                                window.clearFlags(
                                        android.view.WindowManager.LayoutParams
                                                .FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS);
                            }
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
                            // MATCH_PARENT, not explicit pixels: the pre-30
                            // policy keys fullscreen-ness (and with it the
                            // status bar's life and death) off the
                            // MATCH_PARENT constants — explicit pixels
                            // freeze the drawn state on API 29 (proven:
                            // boot-hidden stays hidden, shown stays shown,
                            // while MATCH_PARENT show draws promptly).
                            // MATCH_PARENT resolves against the display
                            // for this IN_SCREEN window, so the frame
                            // stays full-bleed; the runner follows the
                            // live size either way (Round 7.6 rule) and
                            // the settle log proves it.
                            params.width =
                                    android.view.WindowManager.LayoutParams.MATCH_PARENT;
                            params.height =
                                    android.view.WindowManager.LayoutParams.MATCH_PARENT;
                            window.setAttributes(params);
                            if (Build.VERSION.SDK_INT >= 30) {
                                // Book hide recipe on 30+ (immersive
                                // mode): opt out of inset fitting so the
                                // scene stays full-bleed, hide both bar
                                // types, transient reveal on swipe. (On
                                // targetSdk 35+ the platform enforces
                                // edge-to-edge, so the decor-fits call
                                // is a no-op there and load-bearing on
                                // 30-34.)
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
                            android.util.Log.i(
                                    "OppaUi",
                                    "hideBars post: decor=0x"
                                            + Integer.toHexString(view.getSystemUiVisibility())
                                            + " window=0x"
                                            + Integer.toHexString(
                                                    view.getWindowSystemUiVisibility()));
                        } catch (Throwable t) {
                            android.util.Log.e("OppaUi", "hideBars failed", t);
                        }
                    }
                });
    }

    /** Show-half of the runtime screen-mode toggle.
     *
     * <p>Mirrors {@code hideBars} with the hide bits cleared: the
     * window keeps its explicit full-display size, the short-edges
     * cutout mode, and {@code setDecorFitsSystemWindows(false)}, so
     * the scene stays full-bleed and the bars overlay it instead of
     * resizing the native window (no refit owed). The full-bleed layout
     * pair ({@code IN_SCREEN} + {@code NO_LIMITS}) is cleared — pre-30
     * policy keeps the status bar hidden while any of them stands,
     * sysUi bits or not; hide restores the set. The bars persist
     * until the app hides them again ({@code BEHAVIOR_DEFAULT} —
     * shown bars are not transient).
     */
    public static void showBars(final Activity activity) {
        activity.runOnUiThread(
                new Runnable() {
                    @Override
                    public void run() {
                        try {
                            android.view.Window window = activity.getWindow();
                            android.view.View view = window.getDecorView();
                            android.util.Log.i(
                                    "OppaUi",
                                    "showBars pre: decor=0x"
                                            + Integer.toHexString(view.getSystemUiVisibility())
                                            + " window=0x"
                                            + Integer.toHexString(
                                                    view.getWindowSystemUiVisibility()));
                            // Two-step show (pre-30 only the end state
                            // matters, but the intermediate fully-
                            // visible pass resets the transient/hide
                            // session the hide bits armed — immersive
                            // included: a single direct set leaves the
                            // status bar hidden on API 29 while the nav
                            // bar recovers. Same-frame pair, invisible
                            // to the user; the scene ignores insets so
                            // no layout or size moves either way).
                            view.setSystemUiVisibility(
                                    android.view.View.SYSTEM_UI_FLAG_VISIBLE);
                            view.setSystemUiVisibility(
                                    android.view.View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                                            | android.view.View
                                                    .SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                                            | android.view.View
                                                    .SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
                            // The full LAYOUT trio (stock-app shape — the
                            // launcher carries exactly this): layout
                            // extends under both bars while neither hide
                            // bit stands, so both bars draw over the
                            // full-bleed scene. The trio WITHOUT
                            // LAYOUT_FULLSCREEN is the combination the
                            // framework warns against
                            // (LAYOUT_HIDE_NAVIGATION by itself) — on
                            // pre-30 it leaves the status bar undrawn
                            // (proven on API 29: nav recovered, status
                            // never drew). The old "deliberately NOT
                            // set" rule died with the Fullscreen theme:
                            // beside a plain `NoTitleBar` window there is
                            // no fullscreen flag for the layout bit to
                            // conspire with. The window keeps its explicit
                            // full-display pixels either way, so no size
                            // moves.
                            // Pre-30 only: clear the fullscreen flag
                            // hideBars arms (see above) — the plain
                            // `NoTitleBar` theme never sets it, so this
                            // clears only what hide set; without the clear
                            // the flag would pin the status bar hidden
                            // while the sysUi show bits say show.
                            // Untouched on 30+ (never armed there).
                            if (Build.VERSION.SDK_INT < 30) {
                                window.clearFlags(
                                        android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN);
                            }
                            // Full-bleed pair (see hideBars): pre-30
                            // policy keeps the status bar hidden while
                            // these stand. Clearing changes no sizes
                            // (the window keeps its explicit pixels) —
                            // the scene stays full-bleed, the bars
                            // overlay it; hide restores the pair.
                            window.clearFlags(
                                    android.view.WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                                            | android.view.WindowManager.LayoutParams
                                                    .FLAG_LAYOUT_NO_LIMITS);
                            // Fullscreen-layout WITHOUT no-limits (the
                            // stock-app shape: every window that shows
                            // the status bar on pre-30 carries
                            // IN_SCREEN and none carries NO_LIMITS —
                            // the pair-clear above overshot by removing
                            // both).
                            window.addFlags(
                                    android.view.WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN);
                            android.view.WindowManager.LayoutParams params =
                                    window.getAttributes();
                            // Same full-bleed frame as hideBars (see above):
                            // the mode flip must never resize the window.
                            params.layoutInDisplayCutoutMode =
                                    android.view.WindowManager.LayoutParams
                                            .LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                            // MATCH_PARENT, not explicit pixels (see hideBars):
                            // the pre-30 policy keys fullscreen-ness off
                            // the MATCH_PARENT constants, and both halves
                            // use them so neither half can freeze the
                            // drawn state. Geometrically a no-op — the
                            // frame is the full display; and if a device
                            // ever disagrees, the runner follows the live
                            // size (Round 7.6 rule) instead of assuming.
                            params.width =
                                    android.view.WindowManager.LayoutParams.MATCH_PARENT;
                            params.height =
                                    android.view.WindowManager.LayoutParams.MATCH_PARENT;
                            // NOTE: no `params.flags |= ...` here —
                            // hideBars re-asserts NO_LIMITS through
                            // addFlags, but show must not re-add any
                            // flag cleared above: a stale re-add
                            // silently re-hid the status bar on
                            // pre-30 (nav came back, status did not).
                            window.setAttributes(params);
                            // Bar backgrounds (30+ only): the window owns
                            // the bar backgrounds and goes transparent, so
                            // both bars draw over the full-bleed scene
                            // (white icons suit the dark scene top; no
                            // LIGHT bits). Pre-30 this ownership wedges
                            // the status bar drawn once shown (proven on
                            // API 29: nav cycles, status never undraws),
                            // so there the bars keep their stock opaque
                            // backgrounds instead.
                            if (Build.VERSION.SDK_INT >= 30) {
                                window.addFlags(
                                        android.view.WindowManager.LayoutParams
                                                .FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS);
                                window.setStatusBarColor(0);
                                window.setNavigationBarColor(0);
                            }
                            if (Build.VERSION.SDK_INT >= 30) {
                                // Book show recipe on 30+: reveal both
                                // bar types over the full-bleed scene
                                // (decor-fits stays off — same overlay,
                                // never a resize); BEHAVIOR_DEFAULT so
                                // the bars persist until the app hides
                                // them again.
                                window.setDecorFitsSystemWindows(false);
                                android.view.WindowInsetsController controller =
                                        window.getInsetsController();
                                if (controller != null) {
                                    controller.show(
                                            android.view.WindowInsets.Type.navigationBars()
                                                    | android.view.WindowInsets.Type
                                                            .statusBars());
                                    controller.setSystemBarsBehavior(
                                            android.view.WindowInsetsController
                                                    .BEHAVIOR_DEFAULT);
                                }
                            }
                            android.util.Log.i(
                                    "OppaUi",
                                    "showBars post: decor=0x"
                                            + Integer.toHexString(view.getSystemUiVisibility())
                                            + " window=0x"
                                            + Integer.toHexString(
                                                    view.getWindowSystemUiVisibility()));
                        } catch (Throwable t) {
                            android.util.Log.e("OppaUi", "showBars failed", t);
                        }
                    }
                });
    }
}
