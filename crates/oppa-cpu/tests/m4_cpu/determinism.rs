use super::*;

// ---------------------------------------------------------------------------
// 5. Rounded-box determinism: same tree twice → identical boxes and PNG.
// ---------------------------------------------------------------------------

#[test]
fn rounded_box_determinism_same_tree_twice() {
    type BoxesAndPng = (Vec<(f32, f32, f32, f32)>, Vec<u8>);
    fn run_once() -> BoxesAndPng {
        let mut rig = Rig::new("Hi");
        let boxes = rig.host.with_retained_mut(|rec, _| {
            let mut ids = rec.retained_ids();
            ids.sort();
            ids.iter()
                .filter_map(|id| rec.get(*id).and_then(|n| n.layout.clone()))
                .map(|b| (b.x, b.y, b.w, b.h))
                .collect::<Vec<_>>()
        });
        let plan = rig.build_incremental();
        rig.commit_all();
        rig.paint(&plan);
        (boxes, rig.png_bytes())
    }
    let (b1, p1) = run_once();
    let (b2, p2) = run_once();
    assert_eq!(b1, b2, "identical committed boxes");
    assert_eq!(p1, p2, "identical PNG bytes");
}
