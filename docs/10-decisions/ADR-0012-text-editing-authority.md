# ADR-0012: Text-editing authority verdict (b) on Web

Status: Accepted (M1 spike; locked #27–#29; amends #5, #24).
Sources: `spike/REPORT.md` §5; `12-archive/DESIGN.md` §§2.3, 9.2;
[experiment](../11-experiments/text-editing-spike.md).

## Context

Authority model (a) uniform framework ownership vs. (b)
presenter-owned editing with framework-guaranteed behavior —
decidable only empirically.

## Decision

(b) on Web: the DOM backend owns editing authority
(caret/selection/IME/undo) for recognized editable fields; the
renderer contract gains a first-class second text path guaranteeing
behavior via the shared editing-operation suite. Adopted as spec:
commit-on-focus-loss, leading-edge tie-breaks, browser-compatible
word rules (binding the GPU session too), no-CSS-`letter-spacing`
for framework-measured tracked text.

## Alternatives

(a) uniformly — unbuilt on Web (variant A scope-cut); its
Web-specific costs (hidden-input anchoring, framework hit-test
parity, ARIA a11y) remain unmeasured and moot under (b).

## Consequences

Freeze is gated: (a) real-IME delete-range verification — closed
(#28, two hands-off 6/6 PASS runs); (b) bidi/combining/ZWJ corpus —
partial (#29; combining + ZWJ closed, visual ordering deferred to
M3). Presenters confirmed not-stateless (mechanism state incl.
editing sessions).
