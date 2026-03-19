# ATEM-UI

Rust (gpui) UI client for controlling Blackmagic Design ATEM mixers.

This repository is early-stage and currently focuses on an MVP subset:

* Connect to an ATEM switcher over UDP (BURP/BEP) using [`necromancer`](https://github.com/Incomplete-Outputs-Lab/necromancer/tree/feat/streamdeck)
* View and control M/E program/preview sources
* Cut / Auto transitions and next transition type
* Phase 2: AUX source switching and basic DSK controls (Auto / On / Off / Tie / Cut / Fill / Rate)

## Status

Initial UI and core connection/control plumbing is implemented and compiles.
Feature parity with the official “ATEM Software Control” is planned, but not complete yet.

The underlying Rust ATEM implementation is still work-in-progress:
[`necromancer`](https://github.com/Incomplete-Outputs-Lab/necromancer/tree/feat/streamdeck) currently re-implements the protocol and may not cover every command or every switcher model.

## Requirements

* Rust stable toolchain
* The target ATEM switcher must be reachable on the network
* Default UDP control port: `9910`

## Quick start

```bash
cargo run -p atem-ui
```

1. Enter the ATEM IP address (default in UI: `192.168.1.50`)
2. Click `Connect`
3. Select the M/E (ME)
4. Select a source and use:
   * `Set Program` / `Set Preview`
   * `Cut` / `Auto`
   * `Next Transition` + `Set Next Transition`

## Phase 2 (AUX / DSK)

When connected, the UI also provides:

* `AUX -/+` and `Set AUX Source`
* `DSK -/+` and:
  * `DSK Auto`
  * `DSK On` / `DSK Off`
  * `DSK Tie On` / `DSK Tie Off`
  * `DSK Cut -> Sel` / `DSK Fill -> Sel` (uses the selected source)
  * `Rate -/+` + `DSK Rate Set`

## Known limitations

* Exact feature parity with the official ATEM Software Control is not guaranteed yet.
* File upload/download and some advanced behaviors are not implemented in the UI yet.
* “Tally” is currently displayed as part of the internal snapshot, but the UI does not yet fully mirror every tally behavior of the official software.

## Architecture (high level)

* `atem-core`: a thin wrapper around `necromancer`
  * maintains a snapshot (`AtemSnapshot`) from `AtemController` state updates
  * exposes async command methods (set program/preview, transitions, AUX/DSK)
* `atem-ui`: gpui-based desktop app
  * connects using `atem-core`
  * renders current snapshot and sends user actions back to `atem-core`

## License and credits

This project is licensed under the Apache-2.0 license.

Credits:

* ATEM protocol control implementation inspiration and code: `necromancer` (Apache-2.0)
* UI framework: `gpui` / `gpui-component`

