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
4. User selects a result and presses `p` to play with mpv.
5. Optional: press `d` to cache thumbnail for the selected video.

## Minimal Prototype

- Search input + results list + preview panel.
- Enter = search/play, Up/Down = move (Up from top returns to search), `q` = quit.
- Thumbnail support is cache-only in this alpha.

## Requirements

- Rust toolchain (`cargo`, `rustc`)
- `mpv` on PATH

## Build & Run

```bash
cargo build
cargo run
```

## Next Steps

- Render thumbnails inline (viuer/kitty/iterm protocols).
- Async search + thumbnail fetch with cancellation.
- Pagination, history, and caching of search results.
