# homelab-admin — architecture decisions

Phase 3 (tech choice) decided by Kenny on 2026-09-28 (form "Dashboard
techkeuze", answers 12:12). Phase 4 decisions follow below once frozen.

## Inherited, not asked

- Rust server, plain HTML + ES modules in the browser, no build step
  (Phase 0).
- `clippy --all-targets -D warnings`; failures are `Result`, never a panic.
- License `MIT OR Apache-2.0`, like the rest of the homelab workspace.
- Dependency policy of the homelab workspace: small pure-Rust crates may be
  added; anything with C bindings, its own network stack or a heavy
  transitive tree needs a mini-round.
- English UI text (Phase 0).

## Phase 3 decisions

| ID | Decision |
|---|---|
| tech-msrv | The whole workspace moves to `rust-version = "1.97"` (chassis-rs requires it). One pin; the CI MSRV job keeps reading it from `Cargo.toml`. |
| tech-charts | The eight visualisations are drawn by the dashboard's own small SVG modules, coloured with the kp-themes `--chart-*` tokens. No charting library. kp-themes keeps charts out of its scope (its `docs/FEATURES.md`); the modules may move there later. |
| tech-js-checks | Frontend checks mirror kp-themes: JSDoc types checked by `tsc` (`checkJs`, `strict`, no emit), `prettier`, `node --test` for pure modules, Playwright in Chromium and Firefox for pages. |
| tech-kp-intake | kp-themes reaches the dashboard **through chassis-rs**, which already vendors a checksummed subset of kp-themes 7.2.0 (`crates/chassis/static/kp/`, 113 files, `KP_THEMES.sha256`) and serves it under `kp/…`. That subset lacks the component modules the dashboard needs (`wizard.js`, `palette.js`, `datatable.js`, …); chassis-rs is asked to vendor them in 2.3.0. homelab keeps no second copy. (Kenny: "kp-themes zit bij in chassis-rs dus ik zie het probleem niet?") |
| tech-chassis-dep | Depend on a released chassis-rs only: `tag = "v2.3.0"`. Implementation (Phase 5 onwards) waits for that release; Phase 4 can proceed. |
| tech-risk-class | **Recoverable, with a local-first twist** (Kenny's own answer): a small subset at commit time, including the crucial security tests; the full suite runs locally on Kenny's machine before a release, not on GitHub Actions. Fewer, larger commits: development speed matters as much as the tests. |
| tech-platforms | Linux (Garuda), Windows, and an Android smartphone; Chrome and Firefox on each. Every page must work at phone width. |
| tech-envs | Four environments, approved as listed: CT 120 (Debian 13, LAN reach to `homelab-host` 10.10.10.250:8443, own latch credential, data under `/appdata/admin/admin-config`); the development machine (WSL and Garuda: Kenny's full token, cargo, node 26, Playwright browsers in `~/.cache/ms-playwright`); GitHub CI (no LAN, no host, no credentials, mocks only); Kenny's browsers (own theme choice, reduced motion, time zone Europe/Brussels vs UTC in CI, local storage). Phase 7 tests every difference. |

### The security subset at commit time (tech-risk-class)

Named here so the gate can list it; refined in Phase 4 once the modules
exist:

- token scope enforcement (a read token cannot operate, an operate token
  cannot destroy, exec or update the host);
- the destroy confirmation compares the typed stack name;
- secrets never appear in logs, transcripts or error bodies;
- the login gate: nothing under `/app` or `/events` answers without a session;
- LAN-only binding refuses a non-private peer.
