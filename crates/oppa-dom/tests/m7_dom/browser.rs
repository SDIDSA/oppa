#![cfg(windows)]

#[cfg(windows)]
#[allow(clippy::module_inception)] // decision 416 keeps browser::browser paths; the file name matches the lifted module by design.
mod browser {
    use super::super::fields_basic::{render_field, FieldProps};
    use super::super::*;

    fn repo_root() -> std::path::PathBuf {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root above crates/oppa-dom")
            .to_path_buf()
    }

    fn run_node(script: &str) -> (bool, String) {
        let root = repo_root();
        let out = std::process::Command::new("node")
            .arg(format!("spike/web/{script}"))
            .current_dir(&root)
            .output()
            .expect("node must exist for the M7 browser rows");
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn write_results(name: &str, content: &str) {
        let path = repo_root().join("spike").join("results").join(name);
        std::fs::write(&path, content).expect("write parity artifact");
    }

    /// Mixed-coverage shaping must actually split (the test's stated
    /// assumption — Segoe UI has no CJK coverage, so the system
    /// fallback contributes a second face).
    fn mixed_shape(svc: &impl TextService) -> ShapedRun {
        let style = oppa::TextStyle::new("Segoe UI", 16.0);
        let shaped = svc
            .shape("Hello \u{4e16}\u{754c}", &style)
            .expect("shape mixed");
        assert!(
            shaped.runs.len() >= 2,
            "mixed text must shape to 2+ runs: {:?}",
            shaped.runs.iter().map(|r| r.font_id).collect::<Vec<_>>()
        );
        shaped
    }

    #[test]
    fn gpu_per_run_faces_resolve_with_real_faces() {
        use oppa_gpu::atlas::{FaceBytes, FaceTable};
        let probe = oppa_text_dwrite::DWriteTextService::new().expect("dwrite");
        let shaped = mixed_shape(&probe);
        // Deterministic ids (decision 16): the host-side shaping lands
        // on the same ids, so the probe instance resolves every face.
        // Decision 388: faces resolve through the GPU rasterizer's
        // face table (real bytes validate headless — garbage panics
        // loudly in `FaceBytes::checked`); scene encoding coverage
        // lives in the GPU backend's own hardware suite.
        let mut table = FaceTable::default();
        assert!(table.face_for(oppa::FontId(0)).is_none());
        let mut seen_ids: Vec<oppa::FontId> = Vec::new();
        for run in &shaped.runs {
            if seen_ids.contains(&run.font_id) {
                continue;
            }
            seen_ids.push(run.font_id);
            let (face_path, index) = probe
                .font_file_source(run.font_id)
                .unwrap_or_else(|| panic!("font file for {:?}", run.font_id));
            let bytes = std::fs::read(&face_path).expect("read font file");
            assert!(!bytes.is_empty());
            let face = FaceBytes::checked(bytes, index, "m7 real face");
            if seen_ids.len() == 1 {
                table.default = Some(face);
            } else {
                table.explicit.insert(run.font_id, face);
            }
        }
        assert!(table.face_for(oppa::FontId(0)).is_some());
        assert_eq!(table.explicit.len(), seen_ids.len() - 1);

        // Same string through the real pipeline: layout → plan → encode.
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(200.0, 120.0);
        host.mount(
            "Card",
            CardProps {
                title: "Hello \u{4e16}\u{754c}".into(),
            },
            render_card,
        );
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
        let text_op = plan
            .ops
            .iter()
            .find(|op| matches!(op, oppa::DrawOp::Text { .. }))
            .expect("text op");
        let (em_size, fonts, glyphs) = match text_op {
            oppa::DrawOp::Text {
                em_size,
                fonts,
                glyphs,
                ..
            } => (*em_size, fonts.clone(), glyphs.clone()),
            _ => unreachable!(),
        };
        assert_eq!(em_size, 16.0, "exact em, not line_height");
        assert!(fonts.len() >= 2, "segmentation survived: {fonts:?}");
        // Every shaped run resolved a validated face above; the
        // plan carries the same runs the shaper produced.
        for font in &fonts {
            let lo = font.glyph_range.0;
            let hi = font.glyph_range.1;
            assert!(hi <= glyphs.len());
            assert!(
                seen_ids.contains(&font.font_id),
                "run face resolved: {:?}",
                font.font_id
            );
            let _ = &glyphs[lo..hi];
        }
        println!(
            "M7 GPU faces: em={em_size} faces={} glyphs={} runs={}",
            seen_ids.len(),
            glyphs.len(),
            fonts.len()
        );
    }

    /// One parity expectation: (kind, pid, x, y, w, h).
    type ParityCase = (String, String, f32, f32, f32, f32);

    fn parity_scene() -> (ComponentHost, Vec<ParityCase>) {
        // Flat subset, nothing else: explicit-size blocks in a row and
        // a column, one static text line, one ring, one circle. untracked
        // text (the no-CSS-letter-spacing rule is load-bearing here).
        fn render_all(_ctx: &Ctx, _p: &()) -> VNode {
            let a1 = Div("a1")
                .style(Style::new().size(60, 40).bg(Color(0x44_44_44)))
                .build();
            let a2 = Div("a2")
                .style(Style::new().size(80, 40).bg(Color(0x66_66_66)))
                .build();
            let row = Row("rowa")
                .style(
                    Style::new()
                        .size(220, 64)
                        .pad_x(8)
                        .gap(8)
                        .bg(Color(0x22_22_22)),
                )
                .children([a1, a2]);
            let b1 = Div("b1")
                .style(Style::new().size(100, 30).bg(Color(0x44_44_44)))
                .build();
            let b2 = Div("b2")
                .style(Style::new().size(100, 40).bg(Color(0x66_66_66)))
                .build();
            let col = oppa::Column::new()
                .gap(4)
                .style(Style::new().size(120, 90).bg(Color(0x22_22_22)))
                .children([b1, b2]);
            let text: VNode = Text {
                text: Arc::from("Hello world"),
                style: Text::title_small,
            }
            .into();
            let ring = Div("ringd")
                .style(
                    Style::new()
                        .size(60, 30)
                        .radius(8)
                        .bg(Color(0x44_44_44))
                        .border(2, Color(0xAA_BB_CC)),
                )
                .build();
            let disc = Div("disc")
                .style(Style::new().size(40, 40).circle().bg(Color(0x66_66_66)))
                .build();
            Div("stage")
                .style(Style::new().size(360, 400).bg(Color(0xFF_FF_FF)))
                .children([row, col, text, ring, disc])
        }
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_layout_config(oppa::LayoutTextConfig {
            family: "Segoe UI".to_string(),
            ..Default::default()
        });
        host.set_viewport(400.0, 440.0);
        host.mount("All", (), render_all);
        host.run_until_idle();
        // Expectations from the engine itself (committed boxes): the
        // corpus compares ENGINE vs BROWSER, never hand values.
        let cases: Vec<(String, String, f32, f32, f32, f32)> = host.with_retained_mut(|rec, _| {
            let mut out = Vec::new();
            for id in rec.retained_ids() {
                let Some(n) = rec.get(id) else { continue };
                let Some(b) = n.layout.clone() else { continue };
                let pid = format!("{}-{}", id.gen().index(), id.gen().generation());
                if n.tag == oppa::Tag::Text && n.text.is_some() {
                    // Text width parity compares CONTENT (the browser
                    // shrink-wraps auto-width spans; the laid box is
                    // constraint-wide by construction, never comparable).
                    out.push(("text".to_string(), pid, b.x, b.y, b.content_w, b.h));
                } else if n.tag != oppa::Tag::Text {
                    out.push(("box".to_string(), pid, b.x, b.y, b.w, b.h));
                }
            }
            out
        });
        (host, cases)
    }

    fn expected_json(cases: &[(String, String, f32, f32, f32, f32)]) -> String {
        let mut s = String::from("{\"cases\":[");
        for (i, (kind, pid, x, y, w, h)) in cases.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"kind\":\"{kind}\",\"pid\":\"{pid}\",\"x\":{x},\"y\":{y},\"w\":{w},\"h\":{h}}}"
            ));
        }
        s.push_str("]}");
        s
    }

    #[test]
    fn parity_corpus_flat_subset_measured() {
        let (host, cases) = parity_scene();
        assert!(!cases.is_empty());
        let text_rows = cases.iter().filter(|c| c.0 == "text").count();
        assert!(text_rows >= 1, "text widths are corpus rows");
        let mut backend = DomBackend::new(1.0);
        let mut sheet = StyleSheet::new(1.0);
        for d in host.diffs_from(0) {
            use oppa::RendererBackend;
            backend.commit(&d).expect("commit");
        }
        backend.with_sync(&host, &mut sheet).expect("sync");
        write_results("parity_page.html", &render_page("parity", &backend, &sheet));
        write_results("parity_expected.json", &expected_json(&cases));
        let (ok, log) = run_node("parity.mjs");
        println!("{log}");
        assert!(ok, "parity corpus green in the flat subset:\n{log}");
        // The measured numbers for the report-back (match rate + the
        // text-height record rows live in spike/results/parity.json).
        let verdict_path = repo_root()
            .join("spike")
            .join("results")
            .join("parity.json");
        let verdict = std::fs::read_to_string(&verdict_path).expect("parity.json");
        assert!(verdict.contains("\"pass\": true"), "{verdict}");
    }

    #[test]
    fn dom_text_editing_suite_through_real_input() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(400.0, 200.0);
        host.mount(
            "Field",
            FieldProps {
                value: "Hello world".into(),
            },
            render_field,
        );
        host.run_until_idle();
        let mut backend = DomBackend::new(1.0);
        let mut sheet = StyleSheet::new(1.0);
        for d in host.diffs_from(0) {
            use oppa::RendererBackend;
            backend.commit(&d).expect("commit");
        }
        backend.with_sync(&host, &mut sheet).expect("sync");
        write_results("dom_field.html", &render_page("field", &backend, &sheet));
        let (ok, log) = run_node("dom_text.mjs");
        println!("{log}");
        assert!(ok, "editing suite green through the real input:\n{log}");
    }
}
