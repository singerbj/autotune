# 0004 — ASIO behind a feature; control panel via IASIO

**Status:** Accepted

## Context

- The ASIO SDK is proprietary (Steinberg) unless the app is GPLv3; that
  licensing decision is open.
- cpal (the planned ASIO path) does not expose `ASIOControlPanel()`, which
  the `open_asio_panel` command needs.

## Decision

- ASIO lives behind the `asio` cargo feature (`tuner-audio`, forwarded by
  `tuner-engine` and the app). Default builds and releases ship WASAPI only
  until the license is settled; CI builds and lints the feature on every PR.
- Capture tries ASIO first when the selected endpoint maps to an installed
  ASIO driver (vendor-name heuristic), then falls back to WASAPI; the failed
  attempt is shown in the fallback list.
- `open_asio_panel` instantiates the driver's COM object from
  `HKLM\SOFTWARE\ASIO\<driver>\CLSID` and calls `IASIO::controlPanel`
  (vtable index 21) directly.

## Consequences

Enable ASIO in a release with `--features asio` once licensing allows.
