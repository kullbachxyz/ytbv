# ytbv

Fast YouTube terminal UI prototype (alpha). Rust + ratatui + mpv.

## Goals

- Fast TUI for search and playback.
- Minimal latency from query to results.
- Thumbnail support (cache now, inline render later).

## Architecture

### Components

- UI (ratatui + crossterm)
  - Search input, results list, preview panel, status bar.
  - Non-blocking input loop with periodic refresh.

- Search provider (RustyPipe)
  - Uses the `rustypipe` crate to query YouTube's internal API.
  - Results are parsed directly in-process for low latency.

- Thumbnail cache + render
  - Downloads thumbnail bytes to `~/.cache/ytbv/thumbs` (or XDG cache path).
  - Stored for re-use; rendered on the right side of the preview panel.

- Player (mpv)
  - Spawned with `--ytdl-format="bestvideo[height<=1080]+bestaudio/best"`.
  - Non-blocking, leaves the TUI running.

### Data Flow

1. User types query and presses Enter.
2. Background thread invokes RustyPipe and parses results.
3. UI renders results and metadata.
4. User selects a result and presses Enter to play with mpv.

## Usage

- Enter: search (Search) or play (Results).
- Tab / Shift+Tab: move focus forward/backward.
- Up/Down: navigate results when focused.
- `c`: load channel videos when focused on Details.
- `q`: quit.

## Prototype Notes

- Search input + results list + preview panel.
- Thumbnail support uses a disk cache and renders inline when available.

## Requirements

- Rust toolchain (`cargo`, `rustc`)
- `mpv` on PATH (set `YTBV_MPV=/path/to/mpv` if it's not on PATH)

## Build & Run

```bash
cargo build
cargo run
```
## Building from Source

```bash
git clone https://github.com/kullbachxyz/ytbv
cd ytbv
cargo build --release
sudo cp target/release/ytbv /usr/local/bin/
```

## Next Steps

- Improve thumbnail rendering quality and fallbacks.
- Async search + thumbnail fetch with cancellation.
- Pagination, history, and caching of search results.
