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
- `mpv` on PATH (set `YTBV_MPV=/path/to/mpv` if it's not on PATH, e.g. app bundle)

### macOS build notes (sixel support)

`viuer`/`sixel` needs native image libs present. On macOS install and export paths before building:

```bash
brew install libsixel jpeg libpng giflib
export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH}"
export CPATH="/opt/homebrew/include:${CPATH}"
export LIBRARY_PATH="/opt/homebrew/lib:${LIBRARY_PATH}"
# If a previous build failed on sixel-sys, clear it:
cargo clean -p sixel-sys
```

### macOS playback notes (mpv + yt-dlp)

- Install mpv + yt-dlp (Homebrew): `brew install mpv yt-dlp` (or `brew install --cask mpv` for the app bundle).
- If using the .app bundle, point ytbv at it and ensure mpv sees Homebrew’s PATH/yt-dlp. The usual setup is:

```bash
export PATH="/opt/homebrew/bin:$PATH"
export YTBV_MPV=/Applications/mpv.app/Contents/MacOS/mpv
# Optionally force mpv to use a specific yt-dlp path:
export YTBV_MPV_OPTS="--script-opts=ytdl_hook-ytdl_path=/opt/homebrew/bin/yt-dlp"
```

If your shell already has Homebrew’s PATH and mpv finds yt-dlp, the extra options are not needed.

## Build & Run

```bash
cargo build
cargo run
```
## Building from Source

```bash
git clone https://github.com/kullbachxyz/ytbv
cd oxwm
cargo build --release
sudo cp target/release/oxwm /usr/local/bin/
```

## Next Steps

- Render thumbnails inline (viuer/kitty/iterm protocols).
- Async search + thumbnail fetch with cancellation.
- Pagination, history, and caching of search results.
