## Summary

<!-- What changes and why. Cite requirement IDs, e.g. FR-07, NFR-03. -->

## Requirements

- Covers:
- Milestone:

## Checklist

- [ ] Title follows Conventional Commits (`feat(dsp): …`, `fix(audio): …`)
- [ ] Tests added/updated and named after the requirement they verify
- [ ] Real-time rules respected (no alloc/locks/logging/I/O on audio threads; no `unwrap`/`expect` in dsp/audio/engine)
- [ ] `ui/src/bindings.ts` regenerated if commands/events changed (`npm run bindings`)
- [ ] Any conflict with hardware/API limits recorded as an ADR in `docs/decisions/`

## Manual verification (hardware)

<!-- Devices used, measured latency, xruns, Discord check — when relevant. -->
