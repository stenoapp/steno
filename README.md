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

Requirements:

- Rust 1.78+ stable
- Node 20 LTS or later
- pnpm 9+
- Platform build dependencies per the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)

```sh
git clone https://github.com/stenoapp/steno.git
cd steno
pnpm install
pnpm tauri dev
```

For a release build with installer artifacts:

```sh
pnpm tauri build
```

Output lands in `src-tauri/target/release/bundle/`.

## Licence

MIT.
