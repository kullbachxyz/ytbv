use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;
use ratatui::{backend::CrosstermBackend, Frame};
use rustypipe::client::RustyPipe;
use rustypipe::model::{VideoItem, YouTubeItem};
use rustypipe::param::search_filter::SearchFilter;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use time::{format_description, OffsetDateTime};
use viuer::Config as ViuerConfig;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static RUSTYPIPE: OnceLock<RustyPipe> = OnceLock::new();

#[derive(Debug, Clone)]
struct Video {
    title: String,
    url: String,
    channel: Option<String>,
    duration: Option<u64>,
    view_count: Option<u64>,
    publish_date: Option<OffsetDateTime>,
    publish_date_txt: Option<String>,
    thumbnail_url: Option<String>,
    thumbnail_path: Option<PathBuf>,
    thumbnail_size: Option<(u32, u32)>,
    thumbnail_loading: bool,
}

struct App {
    query: String,
    cursor: usize,
    results: Vec<Video>,
    selected: usize,
    status: String,
    rx: Receiver<AppMsg>,
    tx: Sender<AppMsg>,
    searching: bool,
    focus: Focus,
    thumb_area: Option<ratatui::layout::Rect>,
    last_thumb: Option<ThumbRender>,
}

#[derive(Clone)]
struct ThumbRender {
    path: PathBuf,
    area: ratatui::layout::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Results,
}

enum AppMsg {
    Search(Result<Vec<Video>, String>),
    Thumbnail {
        index: usize,
        result: Result<PathBuf, String>,
    },
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();
    let mut app = App {
        query: String::new(),
        cursor: 0,
        results: Vec::new(),
        selected: 0,
        status: "Type a query and press Enter.".to_string(),
        rx,
        tx,
        searching: false,
        focus: Focus::Search,
        thumb_area: None,
        last_thumb: None,
    };

    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(200);

    loop {
        terminal.draw(|f| ui(f, &mut app))?;
        render_thumbnail(&mut app)?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if handle_key(&mut app, key.code)? {
                        break;
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }

        while let Ok(msg) = app.rx.try_recv() {
            match msg {
                AppMsg::Search(result) => {
                    app.searching = false;
                    match result {
                        Ok(results) => {
                            app.results = results;
                            app.selected = 0;
                            if !app.results.is_empty() {
                                app.focus = Focus::Results;
                                let selected = app.selected;
                                queue_thumbnail(&mut app, selected);
                            }
                            app.status = format!("Found {} results.", app.results.len());
                        }
                        Err(err) => {
                            app.status = err;
                        }
                    }
                }
                AppMsg::Thumbnail { index, result } => {
                    if let Some(video) = app.results.get_mut(index) {
                        video.thumbnail_loading = false;
                        match result {
                            Ok(path) => {
                                video.thumbnail_size = thumbnail_size_from_path(&path);
                                video.thumbnail_path = Some(path);
                                app.status = "Thumbnail ready.".to_string();
                            }
                            Err(err) => {
                                app.status = err;
                            }
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_key(app: &mut App, key: KeyCode) -> io::Result<bool> {
    match key {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Enter => {
            match app.focus {
                Focus::Search => {
                    let query = app.query.trim().to_string();
                    if !query.is_empty() && !app.searching {
                        app.searching = true;
                        app.status = format!("Searching for '{query}'...");
                        let tx = app.tx.clone();
                        thread::spawn(move || {
                            let result = search_rustypipe(&query);
                            let _ = tx.send(AppMsg::Search(result));
                        });
                    }
                }
                Focus::Results => {
                    if let Some(video) = app.results.get(app.selected) {
                        play_video(video);
                        app.status = format!("Playing: {}", video.title);
                    }
                }
            }
        }
        KeyCode::Up => {
            if app.focus == Focus::Results {
                if app.selected > 0 {
                    app.selected -= 1;
                    let selected = app.selected;
                    queue_thumbnail(app, selected);
                } else {
                    app.focus = Focus::Search;
                }
            }
        }
        KeyCode::Down => {
            match app.focus {
                Focus::Search => {
                    if !app.results.is_empty() {
                        app.focus = Focus::Results;
                        let selected = app.selected;
                        queue_thumbnail(app, selected);
                    }
                }
                Focus::Results => {
                    if app.selected + 1 < app.results.len() {
                        app.selected += 1;
                        let selected = app.selected;
                        queue_thumbnail(app, selected);
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if app.focus == Focus::Search {
                if app.cursor > 0 {
                    app.cursor -= 1;
                    app.query.remove(app.cursor);
                }
            }
        }
        KeyCode::Left => {
            if app.focus == Focus::Search && app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        KeyCode::Right => {
            if app.focus == Focus::Search && app.cursor < app.query.chars().count() {
                app.cursor += 1;
            }
        }
        KeyCode::Char(c) => {
            if app.focus == Focus::Search {
                app.query.insert(app.cursor, c);
                app.cursor += 1;
            }
        }
        _ => {}
    }

    Ok(false)
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let size = f.size();

    let (preview, _) = match app.results.get(app.selected) {
        Some(video) => {
            let views = video
                .view_count
                .map(format_views)
                .unwrap_or_else(|| "- views".to_string());
            let duration = video
                .duration
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string());
            let uploader = video.channel.clone().unwrap_or_else(|| "-".to_string());
            let published = format_published(video.publish_date_txt.as_deref(), video.publish_date);
            let lines = vec![
                Line::from(Span::styled(
                    &video.title,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(views, Style::default().fg(Color::Yellow))),
                Line::from(Span::styled(
                    format!("Length: {duration}"),
                    Style::default().fg(Color::Green),
                )),
            Line::from(Span::styled(
                format!("Uploaded by {uploader}"),
                Style::default().fg(Color::Blue),
            )),
                Line::from(Span::styled(
                    published,
                    Style::default().fg(Color::LightMagenta),
                )),
            ];
            (Paragraph::new(lines.clone()), lines.len())
        }
        None => {
            let lines = vec![Line::from("No results yet.")];
            (Paragraph::new(lines.clone()), lines.len())
        }
    };

    let preview_height = 10u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(preview_height),
        ])
        .split(size);

    let search_title = "Search";
    let search_block = Block::default().borders(Borders::ALL).title(search_title);
    let search_block = search_block.border_style(match app.focus {
        Focus::Search => Style::default().fg(Color::Cyan),
        Focus::Results => Style::default(),
    });
    let search = Paragraph::new(app.query.as_str()).block(search_block.clone());
    f.render_widget(search, chunks[0]);
    if app.focus == Focus::Search {
        let inner = search_block.inner(chunks[0]);
        if inner.width > 0 {
            let cursor_x = inner.x + app.cursor as u16;
            let cursor_x = cursor_x.min(inner.x + inner.width.saturating_sub(1));
            f.set_cursor(cursor_x, inner.y);
        }
    }

    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, video)| {
            let mut style = Style::default();
            if i == app.selected {
                if app.focus == Focus::Results {
                    style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                }
            }
            ListItem::new(Line::from(Span::styled(video.title.clone(), style)))
        })
        .collect();

    let results_title = "Results";
    let results_block = Block::default()
        .borders(Borders::ALL)
        .title(results_title)
        .border_style(match app.focus {
            Focus::Results => Style::default().fg(Color::Cyan),
            Focus::Search => Style::default(),
        });
    let results = List::new(items).block(results_block);
    f.render_widget(results, chunks[1]);

    let preview_block = Block::default().borders(Borders::ALL).title("Details");
    let preview_inner = preview_block.inner(chunks[2]);
    f.render_widget(preview_block, chunks[2]);

    let (text_area, thumb_area) = match app.results.get(app.selected) {
        Some(video)
            if preview_inner.width >= 50
                && preview_inner.height >= 8
                && video.thumbnail_path.is_some() =>
        {
            let min_text_width = 20;
            let max_thumb_width = preview_inner.width.saturating_sub(min_text_width);
            if max_thumb_width < 10 {
                (preview_inner, None)
            } else {
                let (img_w, img_h) = video.thumbnail_size.unwrap_or((160, 90));
                let (thumb_w, thumb_h) =
                    fit_dimensions_cells(img_w, img_h, max_thumb_width, preview_inner.height);
                if thumb_w == 0 || thumb_h == 0 {
                    (preview_inner, None)
                } else {
                    let x = preview_inner.x + preview_inner.width.saturating_sub(thumb_w);
                    let y = preview_inner.y + preview_inner.height.saturating_sub(thumb_h);
                    let thumb_rect = ratatui::layout::Rect::new(x, y, thumb_w, thumb_h);
                    let text_rect = ratatui::layout::Rect::new(
                        preview_inner.x,
                        preview_inner.y,
                        preview_inner.width.saturating_sub(thumb_w),
                        preview_inner.height,
                    );
                    (text_rect, Some(thumb_rect))
                }
            }
        }
        _ => (preview_inner, None),
    };

    app.thumb_area = thumb_area;
    f.render_widget(preview, text_area);

}

fn search_rustypipe(query: &str) -> Result<Vec<Video>, String> {
    let client = rustypipe_client();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("Failed to create tokio runtime")
    });

    let result = runtime.block_on(
        client
            .query()
            .search_filter(query.to_string(), &SearchFilter::new()),
    );

    let response = match result {
        Ok(response) => response,
        Err(err) => return Err(format!("RustyPipe search failed: {err}")),
    };

    let mut results = Vec::new();
    for item in response.items.items {
        if let YouTubeItem::Video(video) = item {
            results.push(video_item_to_video(video));
        }
    }

    Ok(results)
}

fn play_video(video: &Video) {
    let _ = Command::new("mpv")
        .args([
            "--ytdl-format=bestvideo[height<=1080]+bestaudio/best",
            &video.url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn queue_thumbnail(app: &mut App, index: usize) {
    let tx = app.tx.clone();
    let maybe_url = app.results.get_mut(index).and_then(|video| {
        if video.thumbnail_path.is_none() && !video.thumbnail_loading {
            if let Some(url) = video.thumbnail_url.clone() {
                video.thumbnail_loading = true;
                return Some(url);
            }
        }
        None
    });

    if let Some(url) = maybe_url {
        thread::spawn(move || {
            let result = download_thumbnail(&url);
            let _ = tx.send(AppMsg::Thumbnail { index, result });
        });
    }
}

fn render_thumbnail(app: &mut App) -> io::Result<()> {
    let area = match app.thumb_area {
        Some(area) => area,
        None => {
            app.last_thumb = None;
            return Ok(());
        }
    };

    let video = match app.results.get(app.selected) {
        Some(video) => video,
        None => {
            app.last_thumb = None;
            return Ok(());
        }
    };

    let path = match &video.thumbnail_path {
        Some(path) => path,
        None => {
            app.last_thumb = None;
            return Ok(());
        }
    };

    if let Some(last) = app.last_thumb.as_ref() {
        if last.path == *path && last.area == area {
            return Ok(());
        }
    }

    let config = ViuerConfig {
        x: area.x,
        y: area.y as i16,
        width: Some(u32::from(area.width)),
        height: Some(u32::from(area.height)),
        use_sixel: true,
        ..Default::default()
    };

    let _ = viuer::print_from_file(path, &config);
    app.last_thumb = Some(ThumbRender {
        path: path.clone(),
        area,
    });
    Ok(())
}

fn rustypipe_client() -> &'static RustyPipe {
    RUSTYPIPE.get_or_init(|| {
        let storage_dir = rustypipe_storage_dir();
        RustyPipe::builder()
            .storage_dir(storage_dir)
            .build()
            .expect("Failed to initialize RustyPipe")
    })
}

fn rustypipe_storage_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Path::new(&dir).join("rustypipe");
    }

    if let Ok(home) = std::env::var("HOME") {
        return Path::new(&home).join(".local").join("share").join("rustypipe");
    }

    PathBuf::from(".")
}

fn video_item_to_video(video: VideoItem) -> Video {
    let channel = video.channel.map(|c| c.name);
    let thumbnail_url = video.thumbnail.into_iter().next().map(|t| t.url);
    Video {
        title: video.name,
        url: format!("https://www.youtube.com/watch?v={}", video.id),
        channel,
        duration: video.duration.map(u64::from),
        view_count: video.view_count,
        publish_date: video.publish_date,
        publish_date_txt: video.publish_date_txt,
        thumbnail_url,
        thumbnail_path: None,
        thumbnail_size: None,
        thumbnail_loading: false,
    }
}

fn download_thumbnail(url: &str) -> Result<PathBuf, String> {
    let cache_dir = thumbnail_cache_dir()?;
    fs::create_dir_all(&cache_dir).map_err(|e| format!("Cache dir error: {e}"))?;

    let filename = safe_filename(url);
    let path = cache_dir.join(filename);
    if path.exists() {
        return Ok(path);
    }

    let response = reqwest::blocking::get(url).map_err(|e| format!("Download error: {e}"))?;
    let bytes = response.bytes().map_err(|e| format!("Read error: {e}"))?;
    fs::write(&path, &bytes).map_err(|e| format!("Write error: {e}"))?;

    Ok(path)
}

fn thumbnail_cache_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return Ok(Path::new(&dir).join("ytbv").join("thumbs"));
    }

    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    Ok(Path::new(&home).join(".cache").join("ytbv").join("thumbs"))
}

fn safe_filename(url: &str) -> String {
    let mut name = String::new();
    for c in url.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else {
            name.push('_');
        }
    }
    let max_len = 80;
    if name.len() > max_len {
        name.truncate(max_len);
    }
    format!("{name}.img")
}

fn format_duration(secs: u64) -> String {
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn format_views(views: u64) -> String {
    let (value, suffix) = if views >= 1_000_000_000 {
        (views as f64 / 1_000_000_000.0, "B")
    } else if views >= 1_000_000 {
        (views as f64 / 1_000_000.0, "M")
    } else if views >= 1_000 {
        (views as f64 / 1_000.0, "K")
    } else {
        return format!("{views} views");
    };

    let mut s = format!("{value:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s}{suffix} views")
}

fn format_published(relative: Option<&str>, date: Option<OffsetDateTime>) -> String {
    let absolute = date.and_then(|d| {
        let format = format_description::parse("[day]/[month]/[year]").ok()?;
        d.format(&format).ok()
    });

    match (relative, absolute) {
        (Some(rel), Some(abs)) => format!("Published {rel} [{abs}]"),
        (Some(rel), None) => format!("Published {rel}"),
        (None, Some(abs)) => format!("Published [{abs}]"),
        (None, None) => "Published -".to_string(),
    }
}

fn fit_dimensions_cells(
    img_width: u32,
    img_height: u32,
    bound_width: u16,
    bound_height: u16,
) -> (u16, u16) {
    if img_width == 0 || img_height == 0 || bound_width == 0 || bound_height == 0 {
        return (0, 0);
    }

    let bound_height_px = u32::from(bound_height) * 2;
    if img_width <= u32::from(bound_width) && img_height <= bound_height_px {
        let h = std::cmp::max(1, img_height / 2 + img_height % 2);
        return (img_width as u16, h as u16);
    }

    let ratio = img_width.saturating_mul(bound_height_px);
    let nratio = u32::from(bound_width).saturating_mul(img_height);
    let use_width = nratio <= ratio;
    let intermediate = if use_width {
        img_height.saturating_mul(u32::from(bound_width)) / img_width
    } else {
        img_width.saturating_mul(bound_height_px) / img_height
    };

    if use_width {
        let h = std::cmp::max(1, intermediate / 2);
        (bound_width, h as u16)
    } else {
        let h = std::cmp::max(1, bound_height_px / 2);
        (intermediate as u16, h as u16)
    }
}

fn thumbnail_size_from_path(path: &Path) -> Option<(u32, u32)> {
    match imagesize::size(path) {
        Ok(size) if size.width > 0 && size.height > 0 => {
            Some((size.width as u32, size.height as u32))
        }
        _ => None,
    }
}
