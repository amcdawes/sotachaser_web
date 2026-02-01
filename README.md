# SOTA Chaser Web

Client-only Rust + WASM app that lists SOTA spots and tunes a Kenwood TS-570D via Web Serial (Chromium).

## Requirements
- Rust toolchain
- `wasm32-unknown-unknown` target
- Trunk (`cargo install trunk`)
- Chromium-based browser for Web Serial

## Dev
- `trunk serve`

## Build
- `trunk build --release`

Build output will be in `dist/` (static files for Nginx).

## Notes
- Web Serial requires user permission on first connect.
- CAT commands used:
  - Mode: `MD` (LSB/USB/CW/FM/AM)
  - Frequency: `FA` (11-digit, Hz)

Adjust `tune_kenwood_ts570` in [src/serial.rs](src/serial.rs) if your CAT mapping differs.
