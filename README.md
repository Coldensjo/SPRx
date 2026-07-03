# SPRx

A minimal, fast viewer and extractor for Tibia client files. Open → Search/Find → Export.

Opens a `Tibia.dat` + `Tibia.spr` pair (pick either file — the sibling is found automatically) and browses the client's **things**: items, outfits, effects and missiles, composed from their sprite tiles exactly as the client draws them. A raw sprite grid is also available.

Shares the SpriteForge theme and reuses its `.spr`/`.dat` reading code (`spr_manager.rs` / `dat_reader.rs` / `sprite_protocol.rs`, adapted), but is a fully standalone Tauri app.

## Features

- **Any client version, zero configuration.** The `.spr` header layout (u16 vs u32 sprite count) is detected by validating the address table; the `.dat` version (flag era 7.1–10.9x, patternZ, extended ids, frame durations, frame groups) is detected by trying each parser configuration until one consumes the file exactly to EOF.
- **Things view (the main view).** Categories for Items / Outfits / Effects / Missiles with client-ID search (`2400`, `100-250`, lists) and market-name search where the client has names. Multi-tile things (e.g. 64×64 monsters) are composed from their 32×32 sprites with correct tile anchoring, layers and outfit facing.
- **Details panel.** Always open; each category remembers its selection and defaults to the first thing. Shows the .dat data: dimensions, layers, patterns, frames, flags/properties (light, market, elevation, …) and the underlying sprite IDs. Animated things play their animation (with pause), and outfits get N/E/S/W facing buttons.
- **Export.** Right-click (or use the details panel): composed image as PNG, or a full spritesheet laid out frames × (patterns × layers). The raw sprites tab additionally exports arbitrary selections as sheets.
- **Fast.** Virtualized grids served as PNGs by the Rust backend through a custom `spr://` protocol; only visible cells are ever decoded, one atlas request per grid row (things composed in parallel with rayon). A 13k-item 8.0 client parses in ~17 ms.

## Development

```sh
npm install
npm run tauri:dev
```

## Build

```sh
# Portable .exe only (no installer)
npm run tauri:build:portable

# NSIS installer + portable .exe
npm run tauri:build:all

# NSIS installer only
npm run tauri:build
```

Outputs:

- **Portable**: `src-tauri/target/release/sprx-portable.exe` — single file, no installation required. Copy it anywhere and run.
- **Installer**: `src-tauri/target/release/bundle/nsis/`

## CLI probes

```sh
cargo run --example probe -- <file.spr> [out.png] [start_id]
cargo run --example probe_dat -- <file.dat> <file.spr> [out_dir]
```
