# Third-Party Dependency Licences

Steno's dependency licence policy is defined in [docs/STENO-V1-SPEC.md](docs/STENO-V1-SPEC.md) §12.

**Allowed:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, public domain (Unlicense / CC0).

**Forbidden:** GPL (any version), AGPL, LGPL, SSPL, custom non-standard licences.

This file records the licence of every direct third-party dependency. It is updated whenever a dependency is added or removed. Transitive-dependency verification (via `cargo deny check` and equivalents) is deferred to Milestone 7 (polish & packaging); this file will be regenerated with full coverage at that point.

## Rust crates (direct dependencies — see `src-tauri/Cargo.toml`)

| Crate | Licence | Notes |
|---|---|---|
| `tauri` | MIT OR Apache-2.0 | App framework |
| `tauri-build` | MIT OR Apache-2.0 | Build dep |
| `serde` | MIT OR Apache-2.0 | Serialisation |
| `serde_json` | MIT OR Apache-2.0 | JSON support |
| `cpal` | Apache-2.0 | Cross-platform audio I/O (mic capture) |
| `audiopus` | Apache-2.0 (wraps libopus, BSD-3-Clause) | Opus encoder; `static` feature vendors libopus |
| `ogg` | Apache-2.0 OR MIT | OggOpus muxing |
| `chrono` | Apache-2.0 OR MIT | Date/time formatting for filenames + future frontmatter |
| `dirs` | MIT OR Apache-2.0 | Cross-platform user directories |

## Node packages (direct dependencies — see `package.json`)

| Package | Licence | Notes |
|---|---|---|
| `react` | MIT | UI runtime |
| `react-dom` | MIT | DOM renderer |
| `@tauri-apps/api` | MIT OR Apache-2.0 | Tauri JS bindings |
| `@types/react` | MIT | TS types (dev) |
| `@types/react-dom` | MIT | TS types (dev) |
| `@vitejs/plugin-react` | MIT | Vite React plugin (dev) |
| `typescript` | Apache-2.0 | TS compiler (dev) |
| `vite` | MIT | Build tool (dev) |
| `@tauri-apps/cli` | MIT OR Apache-2.0 | Tauri CLI (dev) |

## Models and binary assets

_None yet — first-run model download flow not implemented._
