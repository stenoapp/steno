# Steno — Vision and Roadmap

**Status:** Draft v0.1
**Owner:** Chirag Patnaik
**Repo:** `github.com/<org-tbd>/steno` (org name resolved at v1.0 kickoff)
**Licence:** MIT
**Not part of:** the NakliTechie portfolio. Steno is its own brand.

---

## What it is

Steno is a desktop meeting recorder and note-taker for macOS, Windows, and Linux. It runs locally. It captures meeting audio (system + mic), transcribes it on-device, and produces structured notes using a local or BYOK language model. Notes are written as plain markdown files to a folder the user picks — by default, Obsidian-compatible.

It will grow toward on-demand screen capture during meetings (v1.1), and eventually continuous capture of audio + screen across the workday (v2). The progression is deliberate: earn trust with the bounded, well-understood meeting case before extending into always-on territory.

---

## Why this exists

There are three good meeting recorders in the OSS landscape today (Meetily, anarlog, James Pember's TUI tool) and one commercial macOS-only player (Talat) that nails the local-first posture. None of them are pan-OS with first-class Linux, none of them have a clean agent face built in from day one, and none of them have made markdown-on-disk the canonical representation. Steno's bet is that those three gaps together are the product.

The deeper bet is on a worldview, not features. Google randomly suspends accounts. Granola stores everything on their servers. Otter trains on your audio unless you opt out. A meeting recorder is, by definition, listening to your most sensitive conversations — sales calls, performance reviews, founder discussions, therapy sessions, family talks. The category should default to local. Steno is the version that does.

The working hypothesis: hardware compute capability continues to grow rapidly, models continue to shrink and improve, and within 18-36 months "everything important about my workday processed locally" becomes table stakes rather than aspirational. Steno is built for that trajectory.

---

## Who it's for

People who would otherwise pay for Granola but won't because their data leaves the machine. Concretely:

- Founders and executives in regulated, competitive, or pre-announcement situations
- Consultants and lawyers whose meeting content is privileged
- Therapists, coaches, and clinicians where confidentiality is statutory
- Engineers and researchers who already live in Obsidian and want meeting notes to land there
- Privacy-conscious individuals in jurisdictions where account-suspension stories scare them

This is not for: the "just give me the cheapest meeting bot" crowd, or anyone whose primary need is real-time live transcription as a closed-caption display. Steno will do those things adequately, but they're not why it exists.

---

## Philosophical posture

These are not preferences. They're the spine. Violating one of these is a sign the product has drifted.

**Local by default, cloud by explicit choice.** Transcription runs on-device. LLM inference runs on-device by default (Ollama / LM Studio). The user can plug in a cloud provider — but it's a deliberate act with a visible indicator, not a hidden default.

**Markdown is the canonical representation.** Every meeting is a markdown file on disk. SQLite exists only as a search index over those files. A user can delete Steno tomorrow and still own everything — readable in any text editor, syncable via any folder-sync mechanism (Syncthing, iCloud Drive, Dropbox, git), indexable by Obsidian without modification.

**No accounts. No telemetry. No cloud relay.** The app does not phone home. There is no analytics endpoint, no "anonymous usage stats," no crash reporting service. If users want to share crash logs, they paste them into GitHub issues. Trust costs more to lose than to gain.

**Agent face from day one.** The app exposes an MCP server locally. Other agents on the user's machine can query meetings, transcripts, and summaries the same way a human can. This is not a v2 feature.

**Pan-OS or it doesn't ship.** Linux is not a second-class target. macOS, Windows, and Linux release simultaneously. If a feature can't ship on all three, it waits.

**Bring your own model.** No bundled LLM. No "Steno-recommended cloud provider." The user picks Ollama, LM Studio, OpenRouter, Anthropic, OpenAI, or anything OpenAI-compatible. Steno is the glue, not the model vendor.

---

## What this is not

Not a Zoom/Meet/Teams bot. Steno never joins meetings. It listens to your machine's audio, the way you would.

Not a SaaS. There will never be a Steno cloud, a Steno account system, or a Steno relay.

Not a transcription service. Transcription is a component of the product, not the product. The product is structured notes you trust enough to act on.

Not a Granola clone aesthetically. Granola's UX is excellent and worth learning from, but Steno's audience values markdown files and keyboard shortcuts more than they value polished onboarding flows.

Not part of NakliTechie. NakliTechie is browser-native, single-file, FSA-first. Steno is native, multi-file, pan-OS. Different shape, different brand, different audience overlap (intersecting but not identical).

---

## Roadmap

### v1.0 — Meeting Recorder (target: 8-12 weeks of agent-led build)

The product is shippable, useful, and complete in its own right at v1.0. Everything below is additive.

- Audio capture: system audio + mic, all three OSes
- Local transcription via whisper.cpp; Parakeet as an option for speed where supported
- Speaker diarization (best-effort; flagged as approximate)
- LLM integration: Ollama, LM Studio, and any OpenAI-compatible endpoint, first-class — not generic
- Auto-detection of running local LLM servers; clear status indicator
- Markdown-as-canonical-store; SQLite as search index only
- Obsidian-compatible folder structure by default; user can override
- Local MCP server exposing meetings, transcripts, summaries
- Signed installers for macOS (.dmg), Windows (.exe / MSI), Linux (AppImage + .deb)
- No telemetry, no accounts, no network calls except to the user-configured LLM endpoint

### v1.1 — On-demand Screen Capture (target: ~6 weeks after v1.0)

- Optional screen recording during a meeting, captured as compressed video alongside the audio
- Frame extraction at meeting end for context (slides, code, dashboards shown)
- OCR over extracted frames; OCR output joins the transcript context for summary generation
- Manual region selection; per-meeting toggle
- All processing remains local

### v1.2 — Templates and Workflow (target: ~4 weeks after v1.1)

- User-defined summary templates (sales call, 1:1, standup, interview, etc.)
- Template selection per meeting or auto-suggestion based on calendar event title
- Action item extraction with structured output
- Optional calendar integration (read-only, local-only: Apple Calendar, Outlook .ics, Google Calendar via local credentials)
- Export adapters: clipboard, file, webhook (user-configured local endpoint)

### v1.3 — Polish and Performance

- Background indexing improvements
- Search-across-all-meetings UI
- Bulk re-transcription with newer/better models
- Audio import (drop in a .mp3, get a meeting note)

### v2 — Continuous Capture (territory, not commitment)

- Optional 24/7 screen + audio capture
- Local indexing and OCR pipeline
- Natural-language query across captured history ("what did Sarah say about the API contract last Tuesday")
- Hard opt-in with visible always-on indicator
- This is a different product shape socially — different default, different trust contract — and is treated as a major release, not an incremental one

---

## What we will not do

These are explicit non-goals. Resist scope-creep toward them.

- **No subscriptions for the core product, ever.** The shape rules this out. A local-first tool that charges monthly is a contradiction the audience will reject.
- **No cloud relay as a Steno service.** If users want hosted inference, they go to OpenRouter or their provider directly. We integrate; we don't intermediate.
- **No meeting-bot mode.** Steno will not join a Zoom/Meet/Teams call as a participant. The category is full of bot-based competitors; Steno's posture is explicitly the opposite.
- **No required online activation.** The app runs offline forever, including any future paid tier. License validation, if it ever exists, is offline-verifiable.
- **No "smart features" that need a cloud.** If a feature can't be done locally, it doesn't ship until it can.

---

## Monetisation (parked, not decided)

Working assumption only — not a commitment, not a v1.0 concern.

The likely shape, when the time comes:

- Source code remains MIT, forever
- Community builds remain free and fully functional, forever
- A "Pro" SKU at $29-49 one-time may eventually exist: signed binaries with auto-update, possibly local video processing components that are dual-licensed
- No subscription model under any condition
- Sponsorware not pursued

Implication for licensing decisions today: any third-party dependency we adopt should be MIT, Apache-2.0, BSD, or similar. GPL/AGPL components are forbidden because they would constrain future paid SKU options. This is a hard rule for the agent.

---

## How success is measured

Not by stars, not by revenue, not by downloads. By a single test: can a user who switched from Granola for privacy reasons say, six months later, that they don't miss it?

If yes, Steno worked.

---

## Open questions to revisit later

- Org name: `getsteno`, `stenoapp`, `steno-hq`, or another. Resolved at v1.0 kickoff based on GitHub + domain availability.
- Code-signing certificates: macOS notarisation requires Apple Developer enrollment ($99/yr); Windows EV cert ~$200-400/yr. Costs are real and should be budgeted before the first paid SKU.
- Linux distribution surfaces: AppImage + .deb are v1.0; Flatpak and Snap are v1.x considerations.
- Whether to publish to Homebrew cask, winget, and similar package managers — likely yes, deferred to v1.1.
