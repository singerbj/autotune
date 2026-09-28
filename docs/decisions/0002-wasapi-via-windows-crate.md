# 0002 — WASAPI through the `windows` crate

**Status:** Accepted

## Context

The architecture lists the `wasapi` crate for exclusive/event-driven streams
and `windows` for IAudioClient3, notifications and AVRT. Using both means two
abstractions over the same `IAudioClient`, and `IAudioClient3` low-latency
shared mode, the buffer-alignment retry for exclusive mode, and precise
error mapping (`AUDCLNT_E_DEVICE_IN_USE`, `AUDCLNT_E_DEVICE_INVALIDATED`) are
easier with direct access.

## Decision

`tuner-audio::wasapi` talks to WASAPI only through the `windows` crate
(same major version as Tauri, 0.62). Each stream thread initialises COM,
opens its client through the fallback chain, joins MMCSS "Pro Audio", and
waits only on its own event handle. Every `unsafe` block carries a
`// SAFETY:` comment (enforced by clippy).

## Consequences

One fewer dependency and full control over the fallback chain. The code is
compile-checked for `x86_64-pc-windows-msvc` in CI; behaviour on real devices
is verified by the manual hardware matrix.
