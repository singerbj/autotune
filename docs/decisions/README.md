# Architecture decision records

Short records of decisions that diverge from, or fill gaps in,
`docs/ARCHITECTURE.md` (as REQUIREMENTS.md asks: "write the conflict into
docs/decisions/ instead of silently diverging").

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-allocation-free-mpm.md) | Own allocation-free McLeod detector instead of `pitch-detection` | Accepted |
| [0002](0002-wasapi-via-windows-crate.md) | WASAPI through the `windows` crate, not the `wasapi` crate | Accepted |
| [0003](0003-vb-cable-distribution.md) | VB-Cable: download at install time unless licensed to bundle | Accepted |
| [0004](0004-asio-feature-and-control-panel.md) | ASIO behind a feature; control panel via `IASIO` directly | Accepted |
| [0005](0005-realtime-diagnostics-via-atomics.md) | Audio threads report through atomics, not a log ring | Accepted |
| [0006](0006-psola-grain-selection-and-latency.md) | PSOLA grain selection and the one-period latency | Accepted |
| [0007](0007-product-name-and-identifier.md) | Product name and identifier | Accepted |
| [0008](0008-vb-cable-buffer-size.md) | VB-Cable internal buffer size is left to the user | Accepted |
| [0009](0009-elevated-as-another-account.md) | Elevated as another account (Administrator Protection) | Accepted |
| [0010](0010-tuning-hotkey-gates-monitoring.md) | The tuning hotkey turns tuning and monitoring on together, off at launch | Accepted |
| [0011](0011-hard-tune-and-vocal-effects.md) | Hard tune, formant shift and a routed vocal effects chain | Accepted |
