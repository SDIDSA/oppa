use super::*;

/// Round 2.1: normalized IME feeds ride the focused session with
/// atomic undo, and the anchor tracks the composition caret.
#[test]
fn ime_preedit_commit_cycle_with_atomic_undo() {
    let (mut loop_, value) = field_harness();
    // Unfocused feeds miss quietly (the type_text precedent).
    assert_eq!(loop_.feed_ime_preedit("ni", Some(2)).expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_commit("你").expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
    assert_eq!(value.get(), SharedString::from(""));
    focus_field(&mut loop_);
    // Preedit overlays the composition — committed value untouched.
    loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
    assert_eq!(value.get(), SharedString::from(""));
    let session = loop_.host().focused_field_session().expect("session");
    assert_eq!(session.composition_text(), "nihao");
    assert!(session.is_composing());
    // Commit applies as one undo unit (pre-composition snapshot).
    loop_.feed_ime_commit("你好").expect("feeds");
    assert_eq!(value.get(), SharedString::from("你好"));
    assert!(!session.is_composing());
    session.undo();
    assert_eq!(value.get(), SharedString::from(""));
}

#[test]
fn ime_cancel_discards_preedit_and_empty_feeds_noop() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_.feed_ime_preedit("ka", Some(2)).expect("feeds");
    let session = loop_.host().focused_field_session().expect("session");
    assert!(session.is_composing());
    loop_.feed_ime_cancel().expect("feeds");
    assert_eq!(value.get(), SharedString::from(""));
    assert!(!session.is_composing());
    // A cancel with nothing composing, an empty preedit (the
    // winit pre-commit clear), and an empty commit all no-op.
    assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_preedit("", None).expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_commit("").expect("feeds"), 0);
    assert!(!session.is_composing());
}

#[test]
fn ime_anchor_tracks_the_composition_caret() {
    let (mut loop_, _) = field_harness();
    assert_eq!(loop_.ime_anchor(), None, "unfocused: no anchor");
    focus_field(&mut loop_);
    // FakeText at body 14px: 8.75px/char; the field-box leaf sits
    // at the origin, so the anchor x is the caret advance.
    loop_.feed_ime_preedit("nihao", Some(2)).expect("feeds");
    let near = loop_.ime_anchor().expect("anchor while composing");
    assert_eq!(near[0], 17.5, "caret x at 2 chars, got {near:?}");
    assert_eq!((near[1], near[3]), (0.0, 14.0), "line box, got {near:?}");
    loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
    let far = loop_.ime_anchor().expect("anchor follows caret");
    assert_eq!(far[0], 43.75, "caret x at 5 chars, got {far:?}");
    assert!(far[0] > near[0], "anchor moves with the caret");
    // After commit the anchor sits at the trailing caret of the
    // committed text (2 FakeText glyphs x 8.75).
    loop_.feed_ime_commit("你好").expect("feeds");
    let done = loop_.ime_anchor().expect("anchor after commit");
    assert_eq!(done[0], 17.5, "trailing caret kept, got {done:?}");
}
