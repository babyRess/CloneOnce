# CloneOnce

CloneOnce is a local macOS app for creating lightweight `.app` wrappers around
apps, commands, files, and folders. It is designed for people who want separate
launch recipes for work profiles, browser profiles, test sessions, or repeatable
command workflows without running a background daemon.

<img src="static/cloneonce-app-icon.svg" width="96" alt="CloneOnce app icon">

## Current Status

CloneOnce `0.1.0` is a functional MVP. It can inspect macOS app bundles, create
basic wrapper apps, and launch them with a local recipe. Generated wrappers are
simple local bundles and are not a full replacement for dedicated multi-instance
launchers yet.

Download the current macOS Apple Silicon build from
[Releases](https://github.com/babyRess/CloneOnce/releases).

## What It Can Do

- Create local `.app` wrappers for macOS apps.
- Create browser profile launchers for Chromium-style and Firefox-style apps.
- Create command wrappers for executables and scripts.
- Create file or folder shortcuts that open through macOS default apps.
- Inspect selected `.app` bundles through `Contents/Info.plist`.
- Detect basic compatibility presets:
  - Chromium `--user-data-dir`
  - Electron isolated `HOME` plus `--user-data-dir`
  - Firefox `-profile`
  - Generic `HOME` override
- Choose a separate data/profile path when the selected strategy uses one.
- Choose an output folder for generated shortcuts.
- Reveal and run generated wrappers from the app.
- Keep the original target app read-only.

## Important Limits

CloneOnce currently generates a zsh-based launcher bundle. It does not yet
provide:

- code-signed or notarized distribution builds
- a compiled helper runner inside generated wrappers
- full login isolation for apps that use shared Keychain access groups or app
  groups; file/profile isolation cannot separate those credentials
- guaranteed Dock ownership for every app
- menu bar controls
- Dock icon effects
- custom shortcut icon compositing
- App Store sandbox/distribution support

Because the release build is not notarized yet, macOS may show a first-run
security prompt. This is expected for the current development release.

## Default Paths

Generated shortcuts are written to:

```text
~/Applications/CloneOnce Shortcuts
```

Default profile data is written to:

```text
~/CloneOnce/Profiles/<Shortcut Name>
```

## Development Requirements

- macOS
- Node.js
- pnpm
- Rust
- Tauri CLI dependencies

Install dependencies:

```sh
pnpm install
```

Run the development app:

```sh
pnpm tauri dev
```

Run the web UI only:

```sh
pnpm dev
```

## Verification

Run the frontend type check:

```sh
pnpm check
```

Build the frontend:

```sh
pnpm build
```

Check Rust:

```sh
cargo check --manifest-path src-tauri/Cargo.toml
```

Run Rust tests:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

Build a local debug bundle:

```sh
pnpm tauri build --debug
```

The debug app and DMG are written under:

```text
src-tauri/target/debug/bundle/
```

## Project Structure

```text
src/routes/+page.svelte      Main seven-screen Svelte UI
src-tauri/src/lib.rs         Tauri commands and wrapper generator
src-tauri/tauri.conf.json    macOS/Tauri app configuration
src-tauri/resources/         Runtime resources for generated wrappers
static/                      Frontend static assets
```

## Privacy

CloneOnce is local-first. It does not include telemetry or a background service.
Generated wrappers are local `.app` bundles that launch the target selected by
the user.

## License

MIT
