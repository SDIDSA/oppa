use super::*;

// ---------------------------------------------------------------------------
// Round 12.1 (decision 307): keyed DOM patches
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct PatchForm {
    value: oppa::Signal<oppa::SharedString>,
    note: oppa::Signal<oppa::SharedString>,
}

fn render_patch_form(_ctx: &Ctx, p: &PatchForm) -> VNode {
    Div("form").children([
        VNode::from(TextField {
            text: p.value.get(),
            style: Text::body_secondary,
            label: oppa::SharedString::from("Name"),
        }),
        VNode::from(Text {
            text: std::sync::Arc::from(p.note.get().to_string()),
            style: Text::body_secondary,
        }),
    ])
}

struct PatchRig {
    host: ComponentHost,
    backend: DomBackend,
    sheet: StyleSheet,
    cursor: usize,
}

impl PatchRig {
    fn form(
        value: &str,
        note: &str,
    ) -> (
        Self,
        oppa::Signal<oppa::SharedString>,
        oppa::Signal<oppa::SharedString>,
    ) {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        host.set_viewport(200.0, 120.0);
        let vs = host.runtime().signal(oppa::SharedString::from(value));
        let ns = host.runtime().signal(oppa::SharedString::from(note));
        host.mount(
            "Form",
            PatchForm {
                value: vs.clone(),
                note: ns.clone(),
            },
            render_patch_form,
        );
        host.run_until_idle();
        let mut rig = Self {
            host,
            backend: DomBackend::new(1.0),
            sheet: StyleSheet::new(1.0),
            cursor: 0,
        };
        rig.sync();
        rig.backend.mark_rendered(&rig.sheet);
        (rig, vs, ns)
    }

    fn sync(&mut self) {
        for d in self.host.diffs_from(self.cursor) {
            self.backend.commit(&d).expect("commit");
        }
        self.cursor = self.host.diff_count();
        self.backend
            .with_sync(&self.host, &mut self.sheet)
            .expect("sync");
    }

    fn field_pid(&self) -> String {
        let id = oppa::find_retained_by_debug(&self.host, "field")[0];
        pid_of(id)
    }
}

/// Decision 392: an unprimed backend drains the whole sheet on
/// the first take (the browser holds no rules yet) — exactly once.
#[test]
fn sheet_delta_drains_exactly_once_and_rides_theme_flips() {
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
    assert!(sheet.rule_count() > 0, "sync interned rules");
    // Unprimed: the whole sheet rides the first take.
    let patch = backend.take_patch(&sheet);
    let shipped = patch.sheet.as_ref().expect("first take ships rules");
    assert_eq!(
        shipped.matches(".s").count(),
        sheet.rule_count(),
        "every rule ships once: {shipped:?}"
    );
    // Second take: settled and duplicate-free.
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert!(patch.sheet.is_none(), "no duplicate rules: {patch:?}");
}

/// Decision 392: a theme flip mints dark-token rules that ride the
/// same take as the stanza (this is the reported kitchen-sink bug —
/// dark classes without rules).
#[test]
fn sheet_delta_carries_theme_flip_rules_with_the_stanza() {
    #[derive(Clone, Default)]
    struct ThemedProps;
    impl oppa::Props for ThemedProps {}
    fn render_themed(ctx: &oppa::Ctx, _: &ThemedProps) -> oppa::VNode {
        oppa::Div("themed")
            .style(
                oppa::Style::new()
                    .size(50.0, 20.0)
                    .bg(ctx.theme().tokens().background),
            )
            .build()
    }
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Themed", ThemedProps, render_themed);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    // Flip host + backend themes together (production
    // `commit_and_sync` publishes both every frame).
    host.set_theme(ThemeMode::Dark);
    host.run_until_idle();
    backend.set_theme_mode(ThemeMode::Dark);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert!(patch.theme.is_some(), "stanza rides");
    let delta = patch.sheet.as_ref().expect("dark rules ride along");
    assert!(
        delta.contains("#121212"),
        "dark page token in the delta: {delta:?}"
    );
    let patch = backend.take_patch(&sheet);
    assert!(patch.is_empty(), "settled again: {patch:?}");
}

/// Settled frames patch nothing (the applier stays idle — no focus
/// disturbance possible). Decision 392: a primed rig (full page
/// rendered via `mark_rendered`) owes no stylesheet delta either.
#[test]
fn patch_empty_when_settled() {
    let (mut rig, _, _) = PatchRig::form("ab", "v1");
    let patch = rig.backend.take_patch(&rig.sheet);
    assert!(patch.is_empty(), "settled take is empty: {patch:?}");
    assert!(patch.theme.is_none(), "settled Light take owes no stanza");
    assert!(patch.sheet.is_none(), "primed rig owes no rules");
    let json = patch.to_json();
    assert!(json.starts_with("{\"v\":1"), "{json}");
    assert!(json.contains("\"swaps\":[]"), "{json}");
    assert!(json.contains("\"sheet\":null"), "{json}");
    assert!(json.ends_with("\"theme\":null,\"sheet\":null}"), "{json}");
}

/// The brief's verification, headless: a background state update
/// (label change) patches exactly the changed span — the focused
/// field's pid appears in NO op, so the browser never touches the
/// input (focus, caret, and IME composition survive by
/// construction).
#[test]
fn patch_background_update_never_touches_the_focused_field() {
    let (mut rig, _, note) = PatchRig::form("ab", "v1");
    let field = rig.field_pid();
    note.set(oppa::SharedString::from("v2"));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch(&rig.sheet);
    assert!(!patch.full, "content change is incremental");
    assert!(patch.removes.is_empty() && patch.places.is_empty());
    assert!(patch.attrs.is_empty() && patch.sels.is_empty() && patch.spacers.is_empty());
    assert_eq!(patch.swaps.len(), 1, "exactly the note span: {patch:?}");
    assert!(patch.swaps[0].1.contains("v2"), "new text rides the swap");
    let json = patch.to_json();
    assert!(
        !json.contains(&format!("\"pid\":\"{field}\"")),
        "no op targets the focused field, {json}"
    );
}

/// A framework-owned value set syncs the live `.value` property
/// (never an element swap while the browser may hold caret).
#[test]
fn patch_framework_value_set_syncs_the_value_property() {
    let (mut rig, value, _) = PatchRig::form("ab", "v1");
    let field = rig.field_pid();
    value.set(oppa::SharedString::from("abc"));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch(&rig.sheet);
    assert!(patch.swaps.is_empty(), "fields never swap: {patch:?}");
    assert_eq!(patch.attrs.len(), 1, "{patch:?}");
    assert_eq!(patch.attrs[0].pid, field);
    assert_eq!(patch.attrs[0].value.as_deref(), Some("abc"));
    // Both channels agree (render carries the attribute too).
    let id = oppa::find_retained_by_debug(&rig.host, "field")[0];
    assert!(
        rig.backend.render_node(id).contains("value=\"abc\""),
        "render and property agree"
    );
}

/// Reorder emits one final-order place (live nodes move in place —
/// the applier never detaches them, so focus survives moves).
#[test]
fn patch_reorder_places_final_order_without_blobs() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Ord", OrderProps { reversed: false }, render_ordered);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    handle.set_props(OrderProps { reversed: true });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert!(!patch.full);
    assert!(patch.removes.is_empty(), "moves, never removals: {patch:?}");
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    assert_eq!(patch.places[0].parent, pid_of(wrap));
    let live: Vec<String> = backend
        .element(wrap)
        .expect("wrap")
        .children
        .iter()
        .map(|c| pid_of(*c))
        .collect();
    assert_eq!(patch.places[0].kids, live, "final order, reversed");
    assert!(
        patch.places[0].blobs.is_empty(),
        "all kids live — pure move"
    );
    // Reordering re-lays-out (Row positions follow order): the two
    // displaced cells swap new geometry, the middle cell is untouched.
    assert_eq!(patch.swaps.len(), 2, "displaced geometry swaps: {patch:?}");
}

/// Added subtrees arrive as blobs on the parent's place (parsed
/// once, appended — existing siblings untouched).
#[test]
fn patch_added_cells_carry_blobs() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 1, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    handle.set_props(CountProps { n: 3, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    assert_eq!(patch.places[0].kids.len(), 3, "final order");
    assert_eq!(patch.places[0].blobs.len(), 2, "two new cells ride blobs");
    for (_, html) in &patch.places[0].blobs {
        assert!(html.contains("data-pid="), "blob is element HTML: {html}");
    }
}

/// Removed subtrees emit topmost only (descendants ride the
/// ancestor removal).
#[test]
fn patch_removed_cells_topmost_only() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 3, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered(&sheet);
    handle.set_props(CountProps { n: 1, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch(&sheet);
    assert_eq!(patch.removes.len(), 2, "two cells gone, topmost: {patch:?}");
    // The survivor's parent still places (child list changed).
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    assert_eq!(patch.places[0].kids.len(), 1);
}

/// Patch transport escapes quotes (HTML rides inside JSON — the
/// `esc` boundary rule's sibling: the HTML escaper entity-encodes
/// first, JSON escaping second, so no raw quote ever breaks the
/// transport).
#[test]
fn patch_json_escapes_embedded_quotes() {
    let (mut rig, _, note) = PatchRig::form("ab", "v1");
    note.set(oppa::SharedString::from("say \"hi\""));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch(&rig.sheet);
    assert_eq!(patch.swaps.len(), 1);
    let json = patch.to_json();
    assert!(
        json.contains("&quot;hi&quot;"),
        "entity-encoded text rides, {json}"
    );
    assert!(
        !json.contains("say \"hi\""),
        "no raw quotes in transport, {json}"
    );
}

#[test]
fn key_events_still_route_beside_scroll() {
    // Scroll/Ime authoring parity (decision 112): nodes declare targets
    // with .on_scroll()/.on_ime() instead of failing loudly.
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Slots", SlotProps { n: 1 }, render_slots);
    host.run_until_idle();
    let list = oppa::find_retained_by_debug(&host, "list")[0];
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 4.0,
    });
    host.inject_input(InputEvent::key(0x0D, KeyState::Pressed));
    host.run_until_idle();
}
