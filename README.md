# ATEM-UI

Tauri v2 desktop app for controlling Blackmagic Design ATEM mixers.

## Current status

- `Tauri v2 + React + TypeScript + Vite` is set up.
- Rust control logic is provided by `atem-core` (wrapper over `necromancer`).
- The UI currently includes an ATEM Software Control-like shell and wiring for:
  - Connect / Disconnect
  - Snapshot sync via Tauri event
  - Program / Preview switching
  - Cut / Auto
  - Next transition selection (`mix`, `dip`, `wipe`)
- Goal remains feature parity with official ATEM Software Control (ongoing).

## Workspace structure

- `atem-core`: ATEM protocol client wrapper and snapshot model.
- `src-tauri`: Tauri Rust host, command handlers, and snapshot event pump.
- `src`: React/TypeScript UI (Vite).

## Requirements

- Rust stable toolchain
- Node.js 18+ and npm
- Reachable ATEM switcher on network
- Default control UDP port: `9910`

## Install

```bash
npm install
```

## Development

```bash
npm run tauri dev
```

## Build (desktop)

```bash
npm run tauri build --debug
```

Generated debug binary (Windows):

`target/debug/atem-ui-tauri.exe`

## Implemented Tauri commands

- `connect(ip, port, reconnect)`
- `disconnect()`
- `get_snapshot()`
- `get_connection_status()`
- `set_program_input_by_index(me, source_index)`
- `set_preview_input_by_index(me, source_index)`
- `cut(me)`
- `auto_transition(me)`
- `set_next_transition(me, transition)`

## Notes and limitations

- ATEM model/command coverage depends on `necromancer`.
- UI parity is still partial; this setup provides the new base architecture.
- Current UI styles are a structural approximation intended for iterative parity work.

## License and credits

Licensed under Apache-2.0.

- Protocol control implementation inspiration/code: `necromancer` (Apache-2.0)
- Desktop framework: `Tauri v2`

