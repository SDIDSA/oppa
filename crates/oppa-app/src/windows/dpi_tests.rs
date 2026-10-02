use super::*;

/// Round 6.2: the DPI-awareness helper reports a known status
/// instead of panicking — first call declares (V2 preferred, v1
/// fallback) or reports refusal; a repeat call (awareness already
/// set) must still answer with a status, never panic.
#[test]
fn dpi_awareness_init_reports_expected_status() {
    fn is_known(s: DpiAwareness) -> bool {
        matches!(
            s,
            DpiAwareness::PerMonitorV2 | DpiAwareness::PerMonitorV1 | DpiAwareness::Unavailable
        )
    }
    assert!(is_known(ensure_dpi_awareness()));
    assert!(is_known(ensure_dpi_awareness()));
}
