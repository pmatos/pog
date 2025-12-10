mod cache;
mod commands;
mod error;
mod file_loader;
mod file_source;
mod remote_loader;
mod search;
mod server;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clap::Parser;
use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Adjustment, Application, ApplicationWindow, Button, CssProvider, Entry, Label, Orientation,
    Overlay, PolicyType, ScrolledWindow, Box as GtkBox, Scrollbar, STYLE_PROVIDER_PRIORITY_APPLICATION,
};

use commands::{CommandResponse, PogCommand};
use file_loader::MappedFile;
use file_source::FileSource;
use remote_loader::RemoteFile;
use search::{SearchDirection, SearchMatch, SearchState};
use server::CommandRequest;

#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub start_col: usize,  // 0-based
    pub end_col: usize,    // exclusive
    pub color: String,
}

#[derive(Debug, Clone, Default)]
pub struct LineMarkings {
    pub full_line_color: Option<String>,
    pub regions: Vec<Region>,
}

impl LineMarkings {
    pub fn is_empty(&self) -> bool {
        self.full_line_color.is_none() && self.regions.is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum FilePath {
    Local(std::path::PathBuf),
    Remote { host: String, path: String },
}

impl FilePath {
    pub fn parse(input: &str) -> Self {
        if let Some(colon_pos) = input.find(':') {
            let potential_host = &input[..colon_pos];
            let potential_path = &input[colon_pos + 1..];

            if potential_path.starts_with('/') && !potential_host.contains('/') {
                return FilePath::Remote {
                    host: potential_host.to_string(),
                    path: potential_path.to_string(),
                };
            }
        }

        FilePath::Local(std::path::PathBuf::from(input))
    }
}

fn parse_file_path(s: &str) -> Result<FilePath, String> {
    Ok(FilePath::parse(s))
}

#[derive(Parser)]
#[command(name = "pog")]
#[command(about = "A fast log file viewer")]
struct Args {
    #[arg(value_parser = parse_file_path)]
    file: FilePath,

    #[arg(long, default_value = "9876", help = "Port for the command server")]
    port: u16,

    #[arg(long, help = "Disable the command server")]
    no_server: bool,
}

const LINES_PER_PAGE: usize = 50;
const SEARCH_BUFFER_LINES: usize = 100;
const SEARCH_HIGHLIGHT_COLOR: &str = "#FFD700";
const SEARCH_CHUNK_SIZE: usize = 1000;

enum FileRequest {
    GetLines {
        start: usize,
        count: usize,
        request_id: u64,
    },
    SearchRange {
        pattern: String,
        start_line: usize,
        end_line: usize,
        request_id: u64,
        navigate_to_first: bool,  // Only navigate to first match on initial search
    },
    FindNextMatch {
        pattern: String,
        from_line: usize,
        direction: SearchDirection,
        request_id: u64,
        // Channel to send back match info (line, col, len) for synchronous socket response
        result_tx: Option<std::sync::mpsc::Sender<Option<(usize, usize, usize)>>>,
    },
}

#[derive(Debug)]
enum FileResponse {
    Lines {
        lines: Vec<(usize, String)>,
        request_id: u64,
        start: usize,
    },
    Error {
        message: String,
    },
    SearchResults {
        matches: Vec<SearchMatch>,
        #[allow(dead_code)]
        request_id: u64,
        searched_range: (usize, usize),
        navigate_to_first: bool,
    },
    FoundMatch {
        #[allow(dead_code)]
        match_info: Option<SearchMatch>,
        line_num: Option<usize>,
        #[allow(dead_code)]
        request_id: u64,
    },
}

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn spawn_file_worker(
    source: Arc<dyn FileSource>,
    request_rx: async_channel::Receiver<FileRequest>,
    response_tx: async_channel::Sender<FileResponse>,
) {
    std::thread::spawn(move || {
        while let Ok(request) = request_rx.recv_blocking() {
            match request {
                FileRequest::GetLines {
                    start,
                    count,
                    request_id,
                } => match source.get_lines(start, count) {
                    Ok(lines) => {
                        let _ = response_tx.send_blocking(FileResponse::Lines {
                            lines,
                            request_id,
                            start,
                        });
                    }
                    Err(e) => {
                        let _ = response_tx.send_blocking(FileResponse::Error {
                            message: e.to_string(),
                        });
                    }
                },
                FileRequest::SearchRange {
                    pattern,
                    start_line,
                    end_line,
                    request_id,
                    navigate_to_first,
                } => {
                    match regex::Regex::new(&pattern) {
                        Ok(regex) => {
                            let count = end_line.saturating_sub(start_line);
                            match source.get_lines(start_line, count) {
                                Ok(lines) => {
                                    let matches = search::search_lines(&regex, &lines);
                                    let _ = response_tx.send_blocking(FileResponse::SearchResults {
                                        matches,
                                        request_id,
                                        searched_range: (start_line, end_line),
                                        navigate_to_first,
                                    });
                                }
                                Err(e) => {
                                    let _ = response_tx.send_blocking(FileResponse::Error {
                                        message: e.to_string(),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = response_tx.send_blocking(FileResponse::Error {
                                message: format!("invalid regex: {}", e),
                            });
                        }
                    }
                }
                FileRequest::FindNextMatch {
                    pattern,
                    from_line,
                    direction,
                    request_id,
                    result_tx,
                } => {
                    match regex::Regex::new(&pattern) {
                        Ok(regex) => {
                            let total_lines = source.line_count();
                            let mut found: Option<SearchMatch> = None;
                            let mut found_line: Option<usize> = None;

                            match direction {
                                SearchDirection::Forward => {
                                    let mut current = from_line + 1;
                                    while current < total_lines && found.is_none() {
                                        let end = (current + SEARCH_CHUNK_SIZE).min(total_lines);
                                        if let Ok(lines) = source.get_lines(current, end - current) {
                                            for (line_num, line) in &lines {
                                                if let Some(mat) = regex.find(line) {
                                                    found = Some(SearchMatch {
                                                        line_num: *line_num,
                                                        start_col: mat.start(),
                                                        end_col: mat.end(),
                                                    });
                                                    found_line = Some(*line_num);
                                                    break;
                                                }
                                            }
                                        }
                                        current = end;
                                    }
                                }
                                SearchDirection::Backward => {
                                    let mut current_end = from_line;
                                    while found.is_none() && current_end > 0 {
                                        let start = current_end.saturating_sub(SEARCH_CHUNK_SIZE);
                                        if let Ok(lines) = source.get_lines(start, current_end - start) {
                                            for (line_num, line) in lines.iter().rev() {
                                                if let Some(mat) = regex.find(line) {
                                                    found = Some(SearchMatch {
                                                        line_num: *line_num,
                                                        start_col: mat.start(),
                                                        end_col: mat.end(),
                                                    });
                                                    found_line = Some(*line_num);
                                                    break;
                                                }
                                            }
                                        }
                                        if start == 0 {
                                            break;
                                        }
                                        current_end = start;
                                    }
                                }
                            }

                            // Send result through sync channel if provided (for socket commands)
                            if let Some(tx) = result_tx {
                                let result = found.as_ref().map(|m| {
                                    (m.line_num, m.start_col, m.end_col - m.start_col)
                                });
                                let _ = tx.send(result);
                            }

                            let _ = response_tx.send_blocking(FileResponse::FoundMatch {
                                match_info: found,
                                line_num: found_line,
                                request_id,
                            });
                        }
                        Err(e) => {
                            // Send error through sync channel if provided
                            if let Some(tx) = result_tx {
                                let _ = tx.send(None);
                            }
                            let _ = response_tx.send_blocking(FileResponse::Error {
                                message: format!("invalid regex: {}", e),
                            });
                        }
                    }
                }
            }
        }
    });
}

fn main() -> glib::ExitCode {
    let args = Args::parse();

    let file_source: Arc<dyn FileSource> = match &args.file {
        FilePath::Local(path) => match MappedFile::open(path) {
            Ok(f) => Arc::new(f),
            Err(e) => {
                eprintln!("Failed to open file: {}", e);
                std::process::exit(1);
            }
        },
        FilePath::Remote { host, path } => match RemoteFile::open(host, path) {
            Ok(f) => Arc::new(f),
            Err(e) => {
                eprintln!("Failed to open remote file: {}", e);
                std::process::exit(1);
            }
        },
    };

    let port = args.port;
    let no_server = args.no_server;

    let app = Application::builder()
        .application_id("com.github.pog")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let file_source_clone = file_source.clone();

    app.connect_activate(move |app| {
        build_ui(app, file_source_clone.clone(), port, no_server);
    });

    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &Application, file_source: Arc<dyn FileSource>, port: u16, no_server: bool) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title(&format!("pog - {}", file_source.display_name()))
        .default_width(1200)
        .default_height(800)
        .build();

    let total_lines = file_source.line_count();
    let file_size = file_source.file_size().unwrap_or(0);

    let (command_tx, command_rx) = async_channel::unbounded::<CommandRequest>();

    if !no_server {
        if let Err(e) = server::start_server(port, command_tx) {
            eprintln!("Failed to start command server: {}", e);
        }
    }

    // CSS provider for styling
    let css_provider = CssProvider::new();
    css_provider.load_from_string(
        ".line-numbers-sidebar { background-color: #2a2a2a; padding-right: 8px; }
         .line-number { color: #888; }
         .search-bar { background-color: rgba(50, 50, 50, 0.95); padding: 8px 16px; border-radius: 0 0 8px 8px; }
         .search-entry { min-width: 300px; }
         .search-info { color: #aaa; margin-left: 8px; margin-right: 8px; }
         .search-close { padding: 4px 8px; }"
    );
    gtk4::style_context_add_provider_for_display(
        &Display::default().expect("Could not get default display"),
        &css_provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Marked lines: line_num (0-based) -> markings (full-line color and/or regions)
    let marked_lines: Rc<RefCell<HashMap<usize, LineMarkings>>> = Rc::new(RefCell::new(HashMap::new()));

    // Search state
    let search_state: Rc<RefCell<SearchState>> = Rc::new(RefCell::new(SearchState::new()));

    // Line numbers sidebar
    let line_numbers_box = GtkBox::new(Orientation::Vertical, 0);
    line_numbers_box.set_width_request(80);
    line_numbers_box.set_css_classes(&["line-numbers-sidebar"]);

    // Separator between line numbers and content
    let separator = gtk4::Separator::new(Orientation::Vertical);

    // Content box for log lines
    let content_box = GtkBox::new(Orientation::Vertical, 0);
    content_box.set_hexpand(true);

    // Horizontal scroll for long lines only
    let h_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .child(&content_box)
        .hexpand(true)
        .vexpand(true)
        .build();

    // Vertical scrollbar - maps directly to line numbers
    // value = first visible line, upper = total lines, page_size = visible lines
    let v_adjustment = Adjustment::new(
        0.0,                           // value (current line)
        0.0,                           // lower
        total_lines as f64,            // upper
        1.0,                           // step increment (1 line)
        LINES_PER_PAGE as f64,         // page increment
        LINES_PER_PAGE as f64,         // page size
    );
    let v_scrollbar = Scrollbar::new(Orientation::Vertical, Some(&v_adjustment));
    v_scrollbar.set_vexpand(true);

    // Layout
    let hbox = GtkBox::new(Orientation::Horizontal, 0);
    hbox.append(&line_numbers_box);
    hbox.append(&separator);
    hbox.append(&h_scroll);
    hbox.append(&v_scrollbar);

    // Search bar UI (overlay)
    let search_box = GtkBox::new(Orientation::Horizontal, 8);
    search_box.set_halign(gtk4::Align::Center);
    search_box.set_valign(gtk4::Align::Start);
    search_box.set_margin_top(10);
    search_box.set_css_classes(&["search-bar"]);
    search_box.set_visible(false);

    let search_entry = Entry::new();
    search_entry.set_placeholder_text(Some("Search regex..."));
    search_entry.set_css_classes(&["search-entry"]);

    let search_info = Label::new(Some(""));
    search_info.set_css_classes(&["search-info"]);

    let search_close_button = Button::with_label("x");
    search_close_button.set_css_classes(&["search-close"]);

    search_box.append(&search_entry);
    search_box.append(&search_info);
    search_box.append(&search_close_button);

    // Overlay to layer search bar over content
    let overlay = Overlay::new();
    overlay.set_child(Some(&hbox));
    overlay.add_overlay(&search_box);

    let current_line: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let latest_request_id: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));

    let (request_tx, request_rx) = async_channel::unbounded::<FileRequest>();
    let (response_tx, response_rx) = async_channel::unbounded::<FileResponse>();

    spawn_file_worker(file_source, request_rx, response_tx);

    // Response handler
    let line_numbers_box_response = line_numbers_box.clone();
    let content_box_response = content_box.clone();
    let current_line_response = current_line.clone();
    let latest_request_id_response = latest_request_id.clone();
    let marked_lines_response = marked_lines.clone();
    let search_state_response = search_state.clone();
    let search_info_response = search_info.clone();
    let v_adjustment_response = v_adjustment.clone();
    let request_tx_response = request_tx.clone();

    glib::spawn_future_local(async move {
        while let Ok(response) = response_rx.recv().await {
            match response {
                FileResponse::Lines {
                    lines,
                    request_id,
                    start,
                } => {
                    let latest = *latest_request_id_response.borrow();
                    // Only display if this is the most recent request
                    if request_id == latest {
                        populate_lines(
                            &line_numbers_box_response,
                            &content_box_response,
                            &lines,
                            &marked_lines_response.borrow(),
                            &search_state_response.borrow(),
                        );
                        *current_line_response.borrow_mut() = start;
                    }
                }
                FileResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
                FileResponse::SearchResults {
                    matches,
                    searched_range,
                    navigate_to_first,
                    ..
                } => {
                    let match_count = matches.len();
                    let first_match_line = {
                        let mut state = search_state_response.borrow_mut();
                        state.update_matches(matches, searched_range);
                        state.current_match().map(|m| m.line_num)
                    };

                    if match_count == 0 {
                        search_info_response.set_text("No matches");
                    } else {
                        search_info_response.set_text(&format!("{} matches", match_count));
                        // Only navigate to first match on initial search, not on re-search
                        if navigate_to_first {
                            if let Some(line) = first_match_line {
                                v_adjustment_response.set_value(line as f64);
                            }
                        }
                    }

                    // Trigger redraw with highlights
                    let start = v_adjustment_response.value() as usize;
                    let request_id = next_request_id();
                    *latest_request_id_response.borrow_mut() = request_id;
                    let _ = request_tx_response.send_blocking(FileRequest::GetLines {
                        start,
                        count: LINES_PER_PAGE,
                        request_id,
                    });
                }
                FileResponse::FoundMatch { line_num, .. } => {
                    if let Some(line) = line_num {
                        search_info_response.set_text(&format!("Match at line {}", line + 1));
                        v_adjustment_response.set_value(line as f64);
                    } else {
                        search_info_response.set_text("No more matches");
                    }
                }
            }
        }
    });

    // Command handler for socket server
    let v_adjustment_cmd = v_adjustment.clone();
    let marked_lines_cmd = marked_lines.clone();
    let request_tx_cmd = request_tx.clone();
    let latest_request_id_cmd = latest_request_id.clone();
    let search_state_cmd = search_state.clone();
    let search_box_cmd = search_box.clone();
    let search_entry_cmd = search_entry.clone();
    let search_info_cmd = search_info.clone();
    glib::spawn_future_local(async move {
        while let Ok(request) = command_rx.recv().await {
            let response = match request.command {
                PogCommand::Goto { line } => {
                    if line == 0 || line > total_lines {
                        CommandResponse::Error(format!(
                            "line out of range: requested {}, file has {} lines",
                            line, total_lines
                        ))
                    } else {
                        let line_0based = (line - 1) as f64;
                        v_adjustment_cmd.set_value(line_0based);
                        CommandResponse::Ok(None)
                    }
                }
                PogCommand::Lines => {
                    CommandResponse::Ok(Some(total_lines.to_string()))
                }
                PogCommand::Top => {
                    let top_line = v_adjustment_cmd.value() as usize + 1;
                    CommandResponse::Ok(Some(top_line.to_string()))
                }
                PogCommand::Size => {
                    CommandResponse::Ok(Some(file_size.to_string()))
                }
                PogCommand::Mark { line, region, color } => {
                    if line == 0 || line > total_lines {
                        CommandResponse::Error(format!(
                            "line out of range: requested {}, file has {} lines",
                            line, total_lines
                        ))
                    } else {
                        let line_0based = line - 1;
                        let mut marks = marked_lines_cmd.borrow_mut();
                        let entry = marks.entry(line_0based).or_default();

                        match region {
                            None => {
                                // Full line mark
                                entry.full_line_color = Some(color);
                            }
                            Some((start, end)) => {
                                // Region mark - convert to 0-based
                                let start_0based = start - 1;
                                let end_0based = end - 1;
                                // Remove overlapping regions
                                entry.regions.retain(|r| r.end_col <= start_0based || r.start_col >= end_0based);
                                entry.regions.push(Region {
                                    start_col: start_0based,
                                    end_col: end_0based,
                                    color,
                                });
                                // Sort regions by start column
                                entry.regions.sort_by_key(|r| r.start_col);
                            }
                        }
                        drop(marks);

                        // Trigger redraw
                        let start = v_adjustment_cmd.value() as usize;
                        let request_id = next_request_id();
                        *latest_request_id_cmd.borrow_mut() = request_id;
                        let _ = request_tx_cmd.send_blocking(FileRequest::GetLines {
                            start,
                            count: LINES_PER_PAGE,
                            request_id,
                        });
                        CommandResponse::Ok(None)
                    }
                }
                PogCommand::Unmark { line, region } => {
                    if line == 0 || line > total_lines {
                        CommandResponse::Error(format!(
                            "line out of range: requested {}, file has {} lines",
                            line, total_lines
                        ))
                    } else {
                        let line_0based = line - 1;
                        let mut marks = marked_lines_cmd.borrow_mut();

                        let removed = match region {
                            None => {
                                // Remove all marks from line
                                marks.remove(&line_0based).is_some()
                            }
                            Some((start, end)) => {
                                // Remove specific region (convert to 0-based)
                                let start_0based = start - 1;
                                let end_0based = end - 1;
                                if let Some(entry) = marks.get_mut(&line_0based) {
                                    let before_len = entry.regions.len();
                                    entry.regions.retain(|r| r.start_col != start_0based || r.end_col != end_0based);
                                    let removed = entry.regions.len() != before_len;
                                    // Clean up empty entries
                                    if entry.is_empty() {
                                        marks.remove(&line_0based);
                                    }
                                    removed
                                } else {
                                    false
                                }
                            }
                        };
                        drop(marks);

                        if removed {
                            // Trigger redraw
                            let start = v_adjustment_cmd.value() as usize;
                            let request_id = next_request_id();
                            *latest_request_id_cmd.borrow_mut() = request_id;
                            let _ = request_tx_cmd.send_blocking(FileRequest::GetLines {
                                start,
                                count: LINES_PER_PAGE,
                                request_id,
                            });
                            CommandResponse::Ok(None)
                        } else {
                            CommandResponse::Error(format!("line {} is not marked", line))
                        }
                    }
                }
                PogCommand::Search { pattern } => {
                    let mut state = search_state_cmd.borrow_mut();
                    match state.set_pattern(&pattern) {
                        Ok(()) => {
                            // Sync UI with socket-initiated search
                            search_box_cmd.set_visible(true);
                            search_entry_cmd.set_text(&pattern);
                            search_info_cmd.set_text("Searching...");

                            let viewport_start = v_adjustment_cmd.value() as usize;
                            let search_start = viewport_start.saturating_sub(SEARCH_BUFFER_LINES);
                            let search_end = (viewport_start + LINES_PER_PAGE + SEARCH_BUFFER_LINES).min(total_lines);
                            drop(state);

                            let _ = request_tx_cmd.send_blocking(FileRequest::SearchRange {
                                pattern,
                                start_line: search_start,
                                end_line: search_end,
                                request_id: next_request_id(),
                                navigate_to_first: true,
                            });

                            // Return OK since search was initiated (results come async)
                            CommandResponse::Ok(None)
                        }
                        Err(e) => CommandResponse::Error(e),
                    }
                }
                PogCommand::SearchNext => {
                    let state = search_state_cmd.borrow();
                    if !state.is_active {
                        CommandResponse::Error("no active search".to_string())
                    } else if state.pattern.is_none() {
                        CommandResponse::Error("no search pattern".to_string())
                    } else {
                        let pattern = state.pattern_str.clone();
                        let current_line = v_adjustment_cmd.value() as usize;
                        drop(state);

                        let (result_tx, result_rx) = std::sync::mpsc::channel();
                        let _ = request_tx_cmd.send_blocking(FileRequest::FindNextMatch {
                            pattern,
                            from_line: current_line,
                            direction: SearchDirection::Forward,
                            request_id: next_request_id(),
                            result_tx: Some(result_tx),
                        });
                        match result_rx.recv() {
                            Ok(Some((line, col, len))) => {
                                CommandResponse::Ok(Some(format!("{} {} {}", line + 1, col + 1, len)))
                            }
                            Ok(None) => CommandResponse::Error("no more matches".to_string()),
                            Err(_) => CommandResponse::Error("search failed".to_string()),
                        }
                    }
                }
                PogCommand::SearchPrev => {
                    let state = search_state_cmd.borrow();
                    if !state.is_active {
                        CommandResponse::Error("no active search".to_string())
                    } else if state.pattern.is_none() {
                        CommandResponse::Error("no search pattern".to_string())
                    } else {
                        let pattern = state.pattern_str.clone();
                        let current_line = v_adjustment_cmd.value() as usize;
                        drop(state);

                        let (result_tx, result_rx) = std::sync::mpsc::channel();
                        let _ = request_tx_cmd.send_blocking(FileRequest::FindNextMatch {
                            pattern,
                            from_line: current_line,
                            direction: SearchDirection::Backward,
                            request_id: next_request_id(),
                            result_tx: Some(result_tx),
                        });
                        match result_rx.recv() {
                            Ok(Some((line, col, len))) => {
                                CommandResponse::Ok(Some(format!("{} {} {}", line + 1, col + 1, len)))
                            }
                            Ok(None) => CommandResponse::Error("no more matches".to_string()),
                            Err(_) => CommandResponse::Error("search failed".to_string()),
                        }
                    }
                }
                PogCommand::SearchClear => {
                    let mut state = search_state_cmd.borrow_mut();
                    state.clear();
                    drop(state);

                    // Sync UI with socket-initiated clear
                    search_box_cmd.set_visible(false);
                    search_entry_cmd.set_text("");
                    search_info_cmd.set_text("");

                    // Trigger redraw to clear highlights
                    let start = v_adjustment_cmd.value() as usize;
                    let request_id = next_request_id();
                    *latest_request_id_cmd.borrow_mut() = request_id;
                    let _ = request_tx_cmd.send_blocking(FileRequest::GetLines {
                        start,
                        count: LINES_PER_PAGE,
                        request_id,
                    });
                    CommandResponse::Ok(None)
                }
            };
            let _ = request.response_tx.send(response);
        }
    });

    // Initial load
    let initial_id = next_request_id();
    *latest_request_id.borrow_mut() = initial_id;
    let _ = request_tx.send_blocking(FileRequest::GetLines {
        start: 0,
        count: LINES_PER_PAGE,
        request_id: initial_id,
    });

    // Scrollbar handler
    let request_tx_scroll = request_tx.clone();
    let latest_request_id_scroll = latest_request_id.clone();
    let search_state_scroll = search_state.clone();

    v_adjustment.connect_value_changed(move |adj| {
        let start_line = adj.value() as usize;
        let request_id = next_request_id();
        *latest_request_id_scroll.borrow_mut() = request_id;

        let _ = request_tx_scroll.send_blocking(FileRequest::GetLines {
            start: start_line,
            count: LINES_PER_PAGE,
            request_id,
        });

        // Re-search if search is active and viewport moved outside searched range
        let state = search_state_scroll.borrow();
        if state.needs_research(start_line, LINES_PER_PAGE, SEARCH_BUFFER_LINES) {
            let pattern = state.pattern_str.clone();
            drop(state);

            let search_start = start_line.saturating_sub(SEARCH_BUFFER_LINES);
            let search_end = (start_line + LINES_PER_PAGE + SEARCH_BUFFER_LINES).min(total_lines);

            let _ = request_tx_scroll.send_blocking(FileRequest::SearchRange {
                pattern,
                start_line: search_start,
                end_line: search_end,
                request_id: next_request_id(),
                navigate_to_first: false,  // Don't navigate on re-search while scrolling
            });
        }
    });

    // Handle mouse wheel scrolling on the content area
    let scroll_controller = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL,
    );
    let v_adjustment_scroll = v_adjustment.clone();
    scroll_controller.connect_scroll(move |_, _, dy| {
        let current = v_adjustment_scroll.value();
        let step = 3.0; // lines per scroll tick
        let new_value = (current + dy * step).clamp(
            v_adjustment_scroll.lower(),
            v_adjustment_scroll.upper() - v_adjustment_scroll.page_size(),
        );
        v_adjustment_scroll.set_value(new_value);
        glib::Propagation::Stop
    });
    h_scroll.add_controller(scroll_controller);

    // Close button handler
    let search_box_close = search_box.clone();
    let search_state_close = search_state.clone();
    let search_info_close = search_info.clone();
    let request_tx_close = request_tx.clone();
    let latest_request_id_close = latest_request_id.clone();
    let v_adjustment_close = v_adjustment.clone();
    search_close_button.connect_clicked(move |_| {
        search_box_close.set_visible(false);
        search_state_close.borrow_mut().clear();
        search_info_close.set_text("");
        // Trigger redraw to clear highlights
        let start = v_adjustment_close.value() as usize;
        let request_id = next_request_id();
        *latest_request_id_close.borrow_mut() = request_id;
        let _ = request_tx_close.send_blocking(FileRequest::GetLines {
            start,
            count: LINES_PER_PAGE,
            request_id,
        });
    });

    // Keyboard controller for search shortcuts
    let key_controller = gtk4::EventControllerKey::new();
    let search_box_key = search_box.clone();
    let search_entry_key = search_entry.clone();
    let search_state_key = search_state.clone();
    let search_info_key = search_info.clone();
    let request_tx_key = request_tx.clone();
    let latest_request_id_key = latest_request_id.clone();
    let v_adjustment_key = v_adjustment.clone();

    key_controller.connect_key_pressed(move |_, key, _code, modifier| {
        use gtk4::gdk::{Key, ModifierType};

        // Ctrl+F to open search
        if modifier.contains(ModifierType::CONTROL_MASK) && key == Key::f {
            search_box_key.set_visible(true);
            search_entry_key.grab_focus();
            return glib::Propagation::Stop;
        }

        // Escape to close search
        if key == Key::Escape && search_box_key.is_visible() {
            search_box_key.set_visible(false);
            search_state_key.borrow_mut().clear();
            search_info_key.set_text("");
            // Trigger redraw to clear highlights
            let start = v_adjustment_key.value() as usize;
            let request_id = next_request_id();
            *latest_request_id_key.borrow_mut() = request_id;
            let _ = request_tx_key.send_blocking(FileRequest::GetLines {
                start,
                count: LINES_PER_PAGE,
                request_id,
            });
            return glib::Propagation::Stop;
        }

        // F3 for next match, Shift+F3 for previous
        if key == Key::F3 {
            let state = search_state_key.borrow();
            if state.is_active && state.pattern.is_some() {
                let pattern = state.pattern_str.clone();
                let current_line = v_adjustment_key.value() as usize;
                drop(state);

                let direction = if modifier.contains(ModifierType::SHIFT_MASK) {
                    SearchDirection::Backward
                } else {
                    SearchDirection::Forward
                };

                let request_id = next_request_id();
                let _ = request_tx_key.send_blocking(FileRequest::FindNextMatch {
                    pattern,
                    from_line: current_line,
                    direction,
                    request_id,
                    result_tx: None,  // UI doesn't need sync response
                });
            }
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    // Search entry activate handler (Enter key)
    let search_state_entry = search_state.clone();
    let search_info_entry = search_info.clone();
    let request_tx_entry = request_tx.clone();
    let v_adjustment_entry = v_adjustment.clone();
    search_entry.connect_activate(move |entry| {
        let pattern = entry.text().to_string();
        if pattern.is_empty() {
            return;
        }

        let mut state = search_state_entry.borrow_mut();
        match state.set_pattern(&pattern) {
            Ok(()) => {
                search_info_entry.set_text("Searching...");
                let viewport_start = v_adjustment_entry.value() as usize;
                let search_start = viewport_start.saturating_sub(SEARCH_BUFFER_LINES);
                let search_end = (viewport_start + LINES_PER_PAGE + SEARCH_BUFFER_LINES).min(total_lines);
                drop(state);

                let request_id = next_request_id();
                let _ = request_tx_entry.send_blocking(FileRequest::SearchRange {
                    pattern,
                    start_line: search_start,
                    end_line: search_end,
                    request_id,
                    navigate_to_first: true,
                });
            }
            Err(e) => {
                search_info_entry.set_text(&e);
            }
        }
    });

    window.set_child(Some(&overlay));
    window.present();
}

#[allow(dead_code)]
fn apply_markings(text: &str, markings: &LineMarkings) -> String {
    let chars: Vec<char> = text.chars().collect();

    // If there's a full-line color and no regions, wrap everything
    if let Some(ref color) = markings.full_line_color {
        if markings.regions.is_empty() {
            return format!(
                "<span background=\"{}\">{}</span>",
                glib::markup_escape_text(color),
                glib::markup_escape_text(text)
            );
        }
    }

    // Build character-level color map
    let mut char_colors: Vec<Option<&str>> = vec![None; chars.len()];

    // Full line color applies to all characters first (as background)
    if let Some(ref color) = markings.full_line_color {
        for slot in &mut char_colors {
            *slot = Some(color.as_str());
        }
    }

    // Region colors override (regions are sorted by start_col)
    for region in &markings.regions {
        for i in region.start_col..region.end_col.min(chars.len()) {
            char_colors[i] = Some(&region.color);
        }
    }

    // Generate markup by grouping consecutive characters with same color
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let current_color = char_colors[i];
        let mut end = i + 1;
        while end < chars.len() && char_colors[end] == current_color {
            end += 1;
        }

        let segment: String = chars[i..end].iter().collect();
        let escaped = glib::markup_escape_text(&segment);

        if let Some(color) = current_color {
            result.push_str(&format!(
                "<span background=\"{}\">",
                glib::markup_escape_text(color)
            ));
            result.push_str(&escaped);
            result.push_str("</span>");
        } else {
            result.push_str(&escaped);
        }

        i = end;
    }

    result
}

fn apply_all_markings(
    text: &str,
    manual_markings: Option<&LineMarkings>,
    search_matches: &[&SearchMatch],
) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    // Build character-level color map with priority:
    // 1. Manual region marks (highest - user explicit)
    // 2. Search highlights (middle)
    // 3. Manual full-line color (lowest - background)
    let mut char_colors: Vec<Option<String>> = vec![None; chars.len()];

    // Full line color applies to all characters first (as background)
    if let Some(markings) = manual_markings {
        if let Some(ref color) = markings.full_line_color {
            for slot in &mut char_colors {
                *slot = Some(color.clone());
            }
        }
    }

    // Apply search highlights
    for search_match in search_matches {
        for i in search_match.start_col..search_match.end_col.min(chars.len()) {
            char_colors[i] = Some(SEARCH_HIGHLIGHT_COLOR.to_string());
        }
    }

    // Manual region marks override search highlights
    if let Some(markings) = manual_markings {
        for region in &markings.regions {
            for i in region.start_col..region.end_col.min(chars.len()) {
                char_colors[i] = Some(region.color.clone());
            }
        }
    }

    // Generate markup by grouping consecutive characters with same color
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let current_color = &char_colors[i];
        let mut end = i + 1;
        while end < chars.len() && char_colors[end] == *current_color {
            end += 1;
        }

        let segment: String = chars[i..end].iter().collect();
        let escaped = glib::markup_escape_text(&segment);

        if let Some(color) = current_color {
            result.push_str(&format!(
                "<span background=\"{}\">",
                glib::markup_escape_text(color)
            ));
            result.push_str(&escaped);
            result.push_str("</span>");
        } else {
            result.push_str(&escaped);
        }

        i = end;
    }

    result
}

fn populate_lines(
    line_numbers_box: &GtkBox,
    content_box: &GtkBox,
    lines: &[(usize, String)],
    marked_lines: &HashMap<usize, LineMarkings>,
    search_state: &SearchState,
) {
    // Clear both boxes
    while let Some(child) = line_numbers_box.first_child() {
        line_numbers_box.remove(&child);
    }
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    // Add lines
    for (line_num, text) in lines {
        // Line number label (sidebar)
        let num_label = Label::new(Some(&format!("{:>8}", line_num + 1)));
        num_label.set_halign(gtk4::Align::End);
        num_label.set_css_classes(&["monospace", "line-number"]);
        line_numbers_box.append(&num_label);

        // Collect search matches for this line
        let search_matches: Vec<&SearchMatch> = if search_state.is_active {
            search_state.viewport_matches
                .iter()
                .filter(|m| m.line_num == *line_num)
                .collect()
        } else {
            Vec::new()
        };

        // Content label with combined markings
        let display_text = apply_all_markings(text, marked_lines.get(line_num), &search_matches);

        let label = Label::new(None);
        if display_text.is_empty() {
            label.set_text("");
        } else {
            label.set_markup(&display_text);
            label.set_use_markup(true);
        }
        label.set_halign(gtk4::Align::Start);
        label.set_selectable(true);
        label.set_css_classes(&["monospace"]);
        content_box.append(&label);
    }
}
