# Steno — v1.0 Agent Handoff

**Status:** Draft v0.1
**Prerequisite reading:** STENO-VISION.md, STENO-V1-SPEC.md
**Audience:** The coding agent (Claude Code or equivalent) building Steno v1.0

---

## 0. How to read this document

This is the operational doc. Vision and spec answered *what* and *why*. This answers *how* and *in what order*. Read this whole document before writing any code. Then keep it open while you work.

You proceed autonomously between milestones. You stop only at the explicit checkpoints listed in §6 and when the conditions in §9 are met.

---

## 1. Repository setup

### 1.1 Org and repo

A new GitHub organisation is created for this project. Likely name: `getsteno`, `stenoapp`, or `steno-hq` — whichever is available at kickoff time. The repo is `<org>/steno`.

This is **not** under `NakliTechie`. It is **not** under Chirag's personal account. Steno has its own home.

### 1.2 Initial repo contents

- `README.md` — short, honest, no marketing
- `LICENSE` — MIT, copyright Chirag Patnaik
- `LICENSES.md` — empty initially, populated as dependencies are added
- `STENO-VISION.md`, `STENO-V1-SPEC.md`, `STENO-V1-AGENT-HANDOFF.md` — copies of these docs in `docs/`
- `.gitignore` — Rust + Node + Tauri standard
- `CONTRIBUTING.md` — to be drafted at v1.0 launch, deferred for now

### 1.3 No CI in milestone 0

CI is set up at the end of milestone 1, not at the start. Don't waste cycles on CI before there's anything to build.

---

## 2. The build environment

Target development on macOS (Chirag's primary), with Linux and Windows builds validated at each milestone via cross-compilation or CI runners (set up in milestone 1).

Required tooling:

- Rust 1.78+ stable
- Node 20 LTS
- pnpm (not npm, not yarn)
- Tauri CLI 2.x
- Platform-specific build dependencies per Tauri docs

The agent verifies the toolchain works by building Tauri's "hello world" template before starting milestone 0.

---

## 3. Hard rules — what NOT to do

Violating any of these is a stop-and-ask event. Do not work around them silently.

1. **Do not add a GPL/AGPL/LGPL/SSPL dependency.** See spec §12. If a feature you need only exists in a copyleft-licensed library, stop and ask before adding it.

2. **Do not import code from Meetily, anarlog, Hyprnote, or Screenpipe.** Reading them is encouraged; copying snippets is not. Inspired-by is fine; copy-paste with rename is not. Write fresh.

3. **Do not add telemetry, analytics, crash reporting services, or "anonymous usage statistics." Not even opt-in.** This is a posture, not a preference. If you think a metrics system would help debugging, build a local-only logging mechanism (spec §9), not a remote one.

4. **Do not add account creation, login, OAuth, or any identity system.** Steno has no concept of a user account.

5. **Do not add features that require a Steno-operated server.** No relay, no cloud sync, no Steno backend. If a feature seems to need one, you've designed it wrong.

6. **Do not break the markdown-as-canonical-store rule.** SQLite never holds content that doesn't also exist in a markdown file. Every code path that writes meeting content writes the markdown first, then updates the index.

7. **Do not include transcript content, audio data, or meeting metadata in log files.** Logs are for operational errors only.

8. **Do not add Electron, web-server-based architectures, or anything that compromises the "native desktop app, three OSes" posture.**

9. **Do not auto-update in v1.0.** Updates are manual downloads. Auto-update is a v1.1 decision.

10. **Do not implement features marked "v1.1+" in v1.0.** Resist scope creep. The spec is the spec.

---

## 4. Architectural priors that are locked

These are not up for redesign during the build. They were decided during the spec phase. If you find yourself wanting to redesign one, that's a stop-and-ask.

1. Tauri 2.x as the app shell (not Electron, not Wails, not native-per-OS)
2. Rust backend, TypeScript frontend
3. Markdown files as canonical store, SQLite as index only
4. Opus/Ogg as audio storage format
5. whisper.cpp as primary transcription engine
6. MCP server exposed over local socket
7. No bundled LLM — Ollama / LM Studio / OpenAI-compatible only
8. Default folder location at `~/Documents/Steno/`
9. No telemetry, no accounts, no auto-update, no cloud relay

Decisions you make autonomously (no need to ask):

- Frontend framework (React vs Svelte vs Solid — pick what works best with Tauri at build time; document the choice)
- Specific Whisper Rust binding (whisper-rs vs direct FFI — pick based on current state)
- Internal module structure within the Rust backend (beyond the top-level layout in spec §13)
- All variable names, function names, file names, internal API shapes
- Specific UI library choices (shadcn/ui, Mantine, vanilla CSS, etc.)
- Test framework and structure
- Debugging strategies, performance optimisations, refactors during the build

---

## 5. Sequencing principle

Build foundations before features. Each milestone produces something demonstrable and either: (a) gates the next milestone, or (b) is independently shippable as a partial release.

The order below is deliberate. Do not skip ahead.

---

## 6. Milestones

### Milestone 0 — Skeleton and pipeline (week 1)

Goal: a Tauri app that launches on all three OSes and displays "Hello Steno." Build pipeline works. CI green.

Deliverables:
- Repo initialised per §1.2
- Tauri app builds and runs on macOS, Linux, Windows
- GitHub Actions workflow that builds on all three OSes on push to main
- README has install-from-source instructions
- LICENSE and LICENSES.md in place

Gate artifact: screenshot of the app running on each OS, plus a green CI run.

**Checkpoint with user before proceeding.**

### Milestone 1 — Audio capture (weeks 2-3)

Goal: the app records system + mic audio to an Opus file on disk.

Deliverables:
- `cpal`-based mic capture on all three OSes
- System audio capture on macOS (ScreenCaptureKit), Windows (WASAPI loopback), Linux (PipeWire)
- Real-time mixing of the two streams to mono
- Opus encoding via `opus` crate, Ogg muxing
- A minimal UI: "Start Recording" / "Stop Recording" buttons, elapsed time, audio level meters
- Permissions handling per OS (mic permission, screen recording permission on macOS)
- Files saved to `~/Documents/Steno/.steno/audio/`

Gate artifact: a 5-minute recording from each OS that plays back cleanly with both system audio and mic audible. Audio level meters work.

**Checkpoint with user before proceeding.**

### Milestone 2 — Transcription (week 4)

Goal: a recording becomes a transcript.

Deliverables:
- First-run model download flow (Whisper base, default)
- whisper.cpp integration via `whisper-rs` or similar
- Transcription runs post-recording; progress shown
- Transcript output as raw text with timestamps
- A "Transcripts" tab in the UI lists past recordings; clicking one shows the transcript

Gate artifact: a 5-minute recording transcribed end-to-end on each OS; transcript quality is comparable to running whisper.cpp directly on the same audio.

### Milestone 3 — Markdown writer + SQLite index (week 5)

Goal: a transcribed recording becomes a markdown file. SQLite indexes it.

Deliverables:
- Markdown writer producing files in the format from spec §2.4 (frontmatter + transcript section; summary section empty for now)
- SQLite schema (spec §2.5) created on first run
- File watcher that reindexes on external edits
- "Rebuild index" command in Settings
- Search bar that does FTS5 search across indexed content
- Folder layout per spec §2.3

Gate artifact: record a meeting, see the .md file on disk, open it in Obsidian, edit it, see the edits reflected after reindex. Search finds known content.

**Checkpoint with user before proceeding.**

### Milestone 4 — LLM integration and summarisation (weeks 6-7)

Goal: transcripts get summarised by an LLM.

Deliverables:
- Auto-detection of Ollama at `localhost:11434` and LM Studio at `localhost:1234`
- Generic OpenAI-compatible endpoint configuration in Settings
- Status indicator showing which provider is configured and reachable
- Summarisation prompt baked in (spec §5.2)
- Summary written to the markdown file in the structured format
- Re-summarise action available in the meeting view
- Failure modes handled per spec §5.3

Gate artifact: a recorded meeting produces a markdown file with summary, key points, action items, decisions, and transcript. Failed LLM calls degrade gracefully.

### Milestone 5 — Diarization (week 8)

Goal: transcripts have speaker labels.

Deliverables:
- `sherpa-onnx` or equivalent integration for speaker segmentation
- Speaker labels appear in transcript ("Speaker 1:", "Speaker 2:", ...)
- Meeting view supports renaming speakers; renames write back to the .md file
- Diarization is best-effort, flagged as approximate in the UI

Gate artifact: a recording with two distinct speakers gets two distinct labels at least most of the time.

### Milestone 6 — MCP server (week 9)

Goal: agents on the machine can query Steno.

Deliverables:
- MCP server over local Unix socket / named pipe
- Tools: `list_meetings`, `get_meeting`, `search_meetings`, `get_action_items`
- Resources: `steno://meeting/{id}` URI scheme
- Server runs whenever the Steno app is running
- Documentation in `docs/MCP.md` with example client connection

Gate artifact: Claude Code (or another MCP client) connects, lists meetings, fetches one, searches them. End-to-end demonstration.

**Checkpoint with user before proceeding.**

### Milestone 7 — Polish and packaging (weeks 10-11)

Goal: shippable installers.

Deliverables:
- macOS: signed and notarised .dmg (requires Apple Developer cert — user provides)
- Windows: signed .exe / MSI installer (requires Windows cert — user provides; if unavailable in v1.0, unsigned with documented "right-click → Run anyway" workaround)
- Linux: AppImage (primary) and .deb (secondary)
- Settings UI polished: folder picker, audio device selection, LLM provider configuration, model management
- Keyboard shortcuts implemented and documented
- First-run experience: pick folder, pick Whisper model size, recommend Ollama/LM Studio installation if neither detected
- Error banners and recovery flows tested
- LICENSES.md complete

Gate artifact: clean installs on each OS from the built artifacts. New user can record their first meeting within 5 minutes of opening the installer.

**Checkpoint with user — this is the v1.0 release decision.**

### Milestone 8 — v1.0 release (week 12)

Goal: tagged v1.0.0 release with binaries on GitHub Releases.

Deliverables:
- GitHub Release with installer binaries for all three OSes
- README updated with download links and quick-start
- Announcement-ready (Chirag handles announcement; agent does not)

---

## 7. What to do when something is ambiguous

The spec is detailed but not exhaustive. When you hit a gap:

1. **If it's a naming or internal-structure decision:** make it. Document briefly in code comments or `docs/DECISIONS.md`.
2. **If it's a feature scope decision:** check whether the spec has any guidance. If it does, follow it. If it doesn't, default to *less* — implement the minimum needed to clear the milestone gate.
3. **If it's an architectural decision that conflicts with §4:** stop and ask.
4. **If it's a "this will be hard but doable" decision:** proceed. The user expects you to push through hard problems autonomously.
5. **If it's a "this seems wrong but I'll do it":** stop and ask. The "this seems wrong" feeling is a signal worth surfacing.

---

## 8. References for technique (re-stated)

Spec §14 lists permissive repos to study. The standing rule is **read, learn, write fresh**. Document significant learnings in `docs/REFERENCES.md` as you go — this is the audit trail proving the codebase is genuinely independent.

Useful study order:

1. Meetily's `src-tauri/` for the overall Tauri + Rust shape and Whisper integration
2. cpal examples for cross-platform audio capture patterns
3. ScreenCaptureKit examples (Apple's docs) for macOS system audio
4. WASAPI loopback examples for Windows
5. PipeWire monitor-source examples for Linux
6. The MCP reference server implementations from Anthropic's docs
7. sherpa-onnx examples for diarization

---

## 9. When to stop and ask

You stop and ask the user when:

1. A milestone checkpoint is reached (marked above)
2. A hard rule (§3) would need to be broken to proceed
3. A locked architectural prior (§4) seems wrong in light of what you've learned
4. A dependency you need has a forbidden licence
5. A new external service or API would need to be introduced
6. You hit a problem you've genuinely been unable to solve after substantial attempt (not "this is annoying" — "I've spent half a day and don't see a path")
7. The "this seems wrong but I'll do it" feeling triggers

You do NOT stop for:

- Naming things
- Choosing between two technically-equivalent implementation approaches
- Debugging
- Refactoring during the build
- Adding tests
- Performance tuning
- UI styling decisions within the established direction

---

## 10. Code-signing and platform secrets

Code signing certificates are a v1.0 concern but a Chirag-provides concern. The agent's job is to ensure the build pipeline can accept signing credentials when they exist. The agent does not need to procure them.

For macOS: the build script accepts Apple Developer ID Application certificate; runs `codesign` and `xcrun notarytool` if credentials are present; produces unsigned .dmg if not.

For Windows: the build script accepts an EV code signing certificate; runs `signtool` if present; produces unsigned .exe if not.

For Linux: AppImage and .deb don't require signing in the same way; nothing to do.

---

## 11. README seed

When the repo is initialised, the README should be honest and short. A starting draft:

```markdown
# Steno

A local-first meeting recorder for macOS, Windows, and Linux.

Records meeting audio (system + mic), transcribes it on your device, and writes structured notes to a folder you choose. No accounts, no cloud, no telemetry.

Notes are markdown files. You own them. They live next to your other notes — Obsidian, vim, VS Code, whatever you use.

Bring your own language model. Steno integrates with Ollama and LM Studio out of the box, or any OpenAI-compatible endpoint.

## Status

Pre-release. v1.0 in progress.

## Install

Coming soon.

## Build from source

[instructions]

## Licence

MIT.
```

No marketing copy beyond this in v1.0. The product earns its words.

---

## 12. Closing posture

This is a real product, not a portfolio exercise. The audience will use it for sensitive meetings — therapy sessions, performance reviews, legal calls. Build it like it matters, because it does.

The architecture is conservative on purpose. The wedge is not technical novelty; it's trust. Every decision should be examined through "does this preserve the user's trust in their own machine."

Ship slowly. Ship right.
