use super::*;

// ---------------------------------------------------------------------------
// Theme contract round (decision 323): page chrome rides a
// change-only stanza; full pages carry the body style.
// ---------------------------------------------------------------------------

/// A toggled theme emits exactly one stanza (change-only, never
/// per-frame noise), the JSON carries it, and the full page styles
/// its own `<body>` — CSS inheritance does the rest, explicit
/// `color:` still wins.
#[test]
fn page_chrome_rides_change_only_stanza_and_body_style() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    // Settled Light: empty, no stanza owed.
    let patch = backend.take_patch(&sheet);
    assert!(patch.is_empty(), "settled Light take is empty");
    assert!(patch.theme.is_none(), "no stanza on repeats");
    // Flip to Dark: the sync counts the page chrome as touched
    // work (a bare toggle on an unthemed tree still emits), and
    // the take carries exactly one stanza.
    backend.set_theme_mode(ThemeMode::Dark);
    let touched = backend.with_sync(&host, &mut sheet).expect("sync").touched;
    assert!(touched > 0, "the flip is sync work");
    let patch = backend.take_patch(&sheet);
    assert!(!patch.is_empty(), "the stanza is patch work");
    let stanza = patch.theme.as_ref().expect("one stanza");
    let dark = ThemeTokens::dark();
    assert_eq!(stanza.bg, "#121212", "tokens drive the wire");
    assert_eq!(stanza.ink, "#f5f5f5", "tokens drive the wire");
    assert!(stanza.dark, "stanza names the dark scheme");
    assert_eq!(dark.background, Color(0x12_12_12));
    assert_eq!(dark.text_primary, Color(0xF5_F5_F5));
    let json = patch.to_json();
    assert!(
        json.contains("\"theme\":{\"bg\":\"#121212\",\"ink\":\"#f5f5f5\",\"dark\":true}"),
        "stanza serializes, {json}"
    );
    // Repeat take: quiet again.
    let patch = backend.take_patch(&sheet);
    assert!(patch.theme.is_none(), "change-only, never per-frame");
    assert!(patch.is_empty(), "settled again");
    // Full pages style their own body (the boot path needs no
    // patch at all).
    let html = render_page("t", &backend, &sheet);
    assert!(
        html.contains("<body style=\"background:#121212;color:#f5f5f5;\">"),
        "dark body style rides the page: {html}"
    );
}

/// Light full pages keep the near-white page + near-black ink the
/// browser implied before (explicit, not browser-default luck).
#[test]
fn light_full_page_styles_its_body() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = render_page("t", &backend, &sheet);
    assert!(
        html.contains("<body style=\"background:#ffffff;color:#111111;\">"),
        "light body style rides the page: {html}"
    );
}

/// Decision 477: a published palette override recolors the page
/// chrome — the stanza carries the override (change-only by value:
/// repeats stay quiet) and the full page styles its own body with
/// it. Unset resolves through the mode exactly as before (the two
/// tests above pin that).
#[test]
fn custom_tokens_recolor_chrome_stanza_and_body() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    let patch = backend.take_patch(&sheet);
    assert!(patch.is_empty(), "settled Light take is empty");
    // Publish custom tokens: one stanza with the override colors.
    let custom = ThemeTokens {
        background: Color(0x19_1D_23),
        text_primary: Color(0xD7_D7_D7),
        ..ThemeTokens::dark()
    };
    backend.set_theme_tokens(custom);
    let touched = backend.with_sync(&host, &mut sheet).expect("sync").touched;
    assert!(touched > 0, "the tokens publish is sync work");
    let patch = backend.take_patch(&sheet);
    let stanza = patch.theme.as_ref().expect("one stanza");
    assert_eq!(stanza.bg, "#191d23", "override drives the wire");
    assert_eq!(stanza.ink, "#d7d7d7", "override drives the wire");
    assert!(!stanza.dark, "mode flag still follows the mode");
    // Repeat publish of identical tokens: quiet again.
    backend.set_theme_tokens(custom);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert!(patch.theme.is_none(), "change-only by value");
    // Full page styles its own body with the override.
    let html = render_page("t", &backend, &sheet);
    assert!(
        html.contains("<body style=\"background:#191d23;color:#d7d7d7;\">"),
        "override body style rides the page: {html}"
    );
}
