# Contributing

## Setup

- Windows 10 22H2+/11 x64 for the full app (Linux works for the core crates,
  the UI, and the app with the mock audio backend).
- Rust — the toolchain is pinned in `rust-toolchain.toml`.
- Node 22 and npm 10.
- On Linux only: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.

```sh
npm install          # installs UI deps and the git hooks (lefthook)
npm run dev          # tauri dev: Vite on :1420 + the Rust app
npm run dev -w ui    # UI only, in a browser, against an in-memory mock
```

## Everyday commands

| Command | What it does |
| --- | --- |
| `npm run check` | UI typecheck/lint/format/test + `cargo fmt --check` + clippy |
| `npm test` | Vitest + `cargo test --workspace` |
| `npm run bindings` | Regenerate `ui/src/bindings.ts` from the Rust commands |
| `cargo run -p tuner-cli -- tune -i in.wav -o out.wav --key A --scale minor` | Offline DSP |
| `cargo bench -p tuner-dsp` | NFR-03 benchmark |
| `cargo test -p tuner-dsp --release -- --ignored soak` | 1-hour offline soak |
| `UPDATE_GOLDEN=1 cargo test -p tuner-cli --test golden` | Regenerate golden files after an intended DSP change |

## Git hooks (lefthook)

- **pre-commit:** `cargo fmt --check`, oxfmt and oxlint on staged UI files,
  and a guard against hand-editing `ui/src/bindings.ts`.
- **commit-msg:** Conventional Commits (`feat(dsp): … (FR-07)`).
- **pre-push:** clippy `-D warnings`, core crate tests, `npm run check -w ui`.

Skip once with `LEFTHOOK=0 git commit …` (CI runs the same checks).

## Pull requests

CI (`.github/workflows/ci.yml`) must be green; the single required check is
**CI success**. It runs:

- PR title lint (Conventional Commits);
- frontend: oxlint, oxfmt, `tsc`, Vitest, Vite build;
- Rust on `windows-latest`: fmt, clippy `-D warnings`, all tests, and a check
  that the generated TS bindings are current;
- core crates on Linux against the mock audio backend;
- the `asio` feature build;
- the NFR-03 benchmark gate and the one-hour `assert_no_alloc` soak;
- `cargo-deny` (licenses, advisories, sources) and an npm license check;
- PSScriptAnalyzer + Pester for the installer scripts;
- an unsigned NSIS build with a silent install → `--restore-defaults` →
  uninstall smoke test.

Follow the hard engineering rules in `docs/REQUIREMENTS.md`; cite requirement
IDs in commits, test names and PRs; record conflicts as ADRs in
`docs/decisions/`.
