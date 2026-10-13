use super::*;

// ---------------------------------------------------------------------------
// Scenario 3 — reload mid-IME-composition (core-side session survival)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ImeProps {
    label: SharedString,
    content: Signal<String>,
    caret: Signal<usize>,
    comp: Signal<Option<String>>,
    ime_hits: Signal<u32>,
}

/// Minimal core-side composition session over the normalized seam: the
/// shape the spike's `EditingSession` proves at full fidelity. Handler
/// writes are signal writes (core-side, residence-safe); nothing here
/// touches TSF machinery (see module honesty notes).
/// Clamps a byte index down to a char boundary (the fuzz writer is
/// byte-oriented while content goes multibyte after the first commit —
/// test-side simplicity, not engine behavior; the spike session steps
/// by cluster).
fn floor_char(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

struct SessionWriter {
    content: Signal<String>,
    caret: Signal<usize>,
    comp: Signal<Option<String>>,
    log: Vec<String>,
}

impl ImeCompositionHandler for SessionWriter {
    fn composition_started(&mut self, start_byte: usize) {
        self.log.push(format!("started@{start_byte}"));
        self.caret.set(start_byte);
        self.comp.set(Some(String::new()));
    }
    fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
        self.log
            .push(format!("updated[{}]@{caret_byte}", composition.len()));
        self.caret.set(caret_byte);
        self.comp.set(Some(composition.to_string()));
    }
    fn composition_committed(&mut self, committed: &str) {
        self.log.push(format!("committed[{}]", committed.len()));
        let mut cur = self.content.get();
        let at = floor_char(&cur, self.caret.get().min(cur.len()));
        cur.insert_str(at, committed);
        self.content.set(cur);
        self.caret.set(at + committed.len());
        self.comp.set(None);
    }
    fn composition_cancelled(&mut self) {
        self.log.push("cancelled".to_string());
        self.comp.set(None);
    }
    fn delete_range(&mut self, range: (usize, usize)) {
        self.log.push(format!("deleted({}-{})", range.0, range.1));
        let mut cur = self.content.get();
        let (a, b) = (
            floor_char(&cur, range.0.min(cur.len())),
            floor_char(&cur, range.1.min(cur.len())),
        );
        if a < b {
            cur.drain(a..b);
            self.content.set(cur);
            self.caret.set(a);
        }
    }
}

mod ime_v1 {
    use super::*;

    #[component]
    pub fn Field(ctx: &Ctx, props: &ImeProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = (local.get(), props.caret.get());
        let shown = match props.comp.get() {
            Some(c) => format!("{}|{c}", props.content.get()),
            None => props.content.get(),
        };
        let _ = props.label.clone();
        Div("field")
            .style(Style::new().size(200, 24))
            .on_ime({
                let hits = props.ime_hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .child(
                Text {
                    text: Arc::from(shown),
                    style: Text::title_small,
                }
                .into(),
            )
    }

    component_manifest![Field(ImeProps)];
}

mod ime_v2 {
    use super::*;

    #[component]
    pub fn Field(ctx: &Ctx, props: &ImeProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = (local.get(), props.caret.get());
        let shown = match props.comp.get() {
            Some(c) => format!("{}|{c}", props.content.get()),
            None => props.content.get(),
        };
        let _ = props.label.clone();
        Div("field")
            .style(Style::new().size(200, 24))
            .on_ime({
                let hits = props.ime_hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .child(
                Text {
                    text: Arc::from(shown),
                    style: Text::title_small,
                }
                .into(),
            )
    }

    component_manifest![Field(ImeProps)];
}

#[test]
fn m9_reload_mid_ime() {
    let mut rng = seed_of("mid-ime", 0x5011_0003);
    const ITERS: usize = 200;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let content = rt.signal(String::from("Hello"));
    let caret = rt.signal(5usize);
    let comp = rt.signal(None::<String>);
    let ime_hits = rt.signal(0u32);
    host.mount(
        "Field",
        ImeProps {
            label: Arc::from("field"),
            content: content.clone(),
            caret: caret.clone(),
            comp: comp.clone(),
            ime_hits: ime_hits.clone(),
        },
        ime_v1::Field,
    );
    host.run_until_idle();
    let field_node = find_retained_by_debug(&host, "field")[0];

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "ime-v1",
        ime_v1::__oppa_manifest_descs(),
    )));

    let mut writer = SessionWriter {
        content: content.clone(),
        caret: caret.clone(),
        comp: comp.clone(),
        log: Vec::new(),
    };
    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut swaps_mid_comp = 0u32;
    let mut sequences = 0u32;
    let mut events_fed = 0u64;

    // Scripted composition alphabet (zh candidate commit, ja
    // kana→candidate, cancel-mid, delete-range re-anchor — the M1
    // scenario shapes, headless through the normalized seam).
    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 8;
        match op {
            // 0-3: run one composition sequence; swap mid-sequence
            // (after Started/Updated, before Commit/Cancel) with p≈1/2.
            0..=3 => {
                sequences += 1;
                let start = (xorshift(&mut rng) % 6) as usize;
                let seq: Vec<ImeCompositionEvent> = match xorshift(&mut rng) % 4 {
                    0 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "ni".to_string(),
                            caret_byte: start + 2,
                        },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "nihao".to_string(),
                            caret_byte: start + 5,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "你好".to_string(),
                        },
                    ],
                    1 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "konnitiha".to_string(),
                            caret_byte: start + 9,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "こんにちは".to_string(),
                        },
                    ],
                    2 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "ka".to_string(),
                            caret_byte: start + 2,
                        },
                        ImeCompositionEvent::CompositionCancelled,
                    ],
                    _ => vec![
                        ImeCompositionEvent::DeleteRange { range: (0, start) },
                        ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "o".to_string(),
                            caret_byte: 1,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "o".to_string(),
                        },
                    ],
                };
                let mid = seq.len() / 2;
                for (k, ev) in seq.iter().enumerate() {
                    dispatch_ime_event(&mut writer, ev);
                    events_fed += 1;
                    if k + 1 == mid && xorshift(&mut rng).is_multiple_of(2) {
                        // Mid-composition swap: the buffer must survive.
                        let buf_before = comp.get();
                        let log_before = writer.log.len();
                        use_v2 = !use_v2;
                        let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                            Box::new(StaticSource::new("ime-v2", ime_v2::__oppa_manifest_descs()))
                        } else {
                            Box::new(StaticSource::new("ime-v1", ime_v1::__oppa_manifest_descs()))
                        };
                        let rep = reg.reload_to(src);
                        assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                        retired.push(rep.outgoing);
                        swaps += 1;
                        swaps_mid_comp += 1;
                        host.run_until_idle();
                        assert_eq!(
                            comp.get(),
                            buf_before,
                            "iter {i}: composition buffer lost across swap"
                        );
                        assert_eq!(
                            writer.log.len(),
                            log_before,
                            "iter {i}: canonical stream disturbed by swap"
                        );
                        assert_no_retired_props(&host, &retired, "mid-ime");
                    }
                }
                host.run_until_idle();
                // Session readable post-sequence (no retired touch).
                let _ = (content.get(), caret.get(), comp.get());
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
            // 4: route an Ime input event at the field (kind-routing
            // survives swaps — handler re-registration via re-runs).
            4 => {
                let before = ime_hits.get();
                host.inject_input(InputEvent::Ime { target: field_node });
                host.run_until_idle();
                assert_eq!(ime_hits.get(), before + 1, "iter {i}: Ime route lost");
            }
            // 5: swap at rest.
            5 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new("ime-v2", ime_v2::__oppa_manifest_descs()))
                } else {
                    Box::new(StaticSource::new("ime-v1", ime_v1::__oppa_manifest_descs()))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                swaps += 1;
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
            // 6-7: settle + invariants.
            _ => {
                host.run_until_idle();
                let _ = (content.get(), caret.get(), comp.get());
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
        }
    }

    eprintln!(
        "m9[mid-ime] iters={ITERS} sequences={sequences} events={events_fed} \
         swaps={swaps} mid_composition={swaps_mid_comp}"
    );
    assert!(sequences >= 40, "too few composition sequences");
    assert!(swaps >= 15, "too few swaps");
    assert!(
        swaps_mid_comp >= 10,
        "no swap landed mid-composition — hardest-case timing vacuous"
    );
    assert!(events_fed >= 200, "too few composition events fed");
}
