mod cache;
mod commands;
mod error;
mod file_loader;
mod file_source;
mod remote_loader;
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
    Adjustment, Application, ApplicationWindow, CssProvider, Label, Orientation, PolicyType,
    ScrolledWindow, Box as GtkBox, Scrollbar, STYLE_PROVIDER_PRIORITY_APPLICATION,
};

use commands::{CommandResponse, PogCommand};
use file_loader::MappedFile;
use file_source::FileSource;
use remote_loader::RemoteFile;
use server::CommandRequest;

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

#[derive(Debug)]
enum FileRequest {
    GetLines {
        start: usize,
        count: usize,
        request_id: u64,
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

    // CSS provider for dynamic line marking
    let css_provider = CssProvider::new();
    css_provider.load_from_string("");
    gtk4::style_context_add_provider_for_display(
        &Display::default().expect("Could not get default display"),
        &css_provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Marked lines: line_num (0-based) -> color
    let marked_lines: Rc<RefCell<HashMap<usize, String>>> = Rc::new(RefCell::new(HashMap::new()));

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
    hbox.append(&h_scroll);
    hbox.append(&v_scrollbar);

    let current_line: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let latest_request_id: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));

    let (request_tx, request_rx) = async_channel::unbounded::<FileRequest>();
    let (response_tx, response_rx) = async_channel::unbounded::<FileResponse>();

    spawn_file_worker(file_source, request_rx, response_tx);

    // Response handler
    let content_box_response = content_box.clone();
    let current_line_response = current_line.clone();
    let latest_request_id_response = latest_request_id.clone();
    let marked_lines_response = marked_lines.clone();

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
                        populate_lines(&content_box_response, &lines, &marked_lines_response.borrow());
                        *current_line_response.borrow_mut() = start;
                    }
                }
                FileResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }
    });

    // Command handler for socket server
    let v_adjustment_cmd = v_adjustment.clone();
    let marked_lines_cmd = marked_lines.clone();
    let css_provider_cmd = css_provider.clone();
    let request_tx_cmd = request_tx.clone();
    let latest_request_id_cmd = latest_request_id.clone();
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
                PogCommand::Mark { line, color } => {
                    if line == 0 || line > total_lines {
                        CommandResponse::Error(format!(
                            "line out of range: requested {}, file has {} lines",
                            line, total_lines
                        ))
                    } else {
                        let line_0based = line - 1;
                        marked_lines_cmd.borrow_mut().insert(line_0based, color);
                        update_marked_css(&css_provider_cmd, &marked_lines_cmd.borrow());
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
                PogCommand::Unmark { line } => {
                    if line == 0 || line > total_lines {
                        CommandResponse::Error(format!(
                            "line out of range: requested {}, file has {} lines",
                            line, total_lines
                        ))
                    } else {
                        let line_0based = line - 1;
                        if marked_lines_cmd.borrow_mut().remove(&line_0based).is_some() {
                            update_marked_css(&css_provider_cmd, &marked_lines_cmd.borrow());
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

    v_adjustment.connect_value_changed(move |adj| {
        let start_line = adj.value() as usize;
        let request_id = next_request_id();
        *latest_request_id_scroll.borrow_mut() = request_id;

        let _ = request_tx_scroll.send_blocking(FileRequest::GetLines {
            start: start_line,
            count: LINES_PER_PAGE,
            request_id,
        });
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

    window.set_child(Some(&hbox));
    window.present();
}

fn update_marked_css(css_provider: &CssProvider, marked_lines: &HashMap<usize, String>) {
    let mut css = String::new();
    for (line_num, color) in marked_lines {
        css.push_str(&format!(
            ".marked-line-{} {{ background-color: {}; }}\n",
            line_num, color
        ));
    }
    css_provider.load_from_string(&css);
}

fn populate_lines(content_box: &GtkBox, lines: &[(usize, String)], marked_lines: &HashMap<usize, String>) {
    // Clear
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    // Add lines
    for (line_num, text) in lines {
        let label = Label::new(Some(&format!("{:8} │ {}", line_num + 1, text)));
        label.set_halign(gtk4::Align::Start);
        label.set_selectable(true);
        if marked_lines.contains_key(line_num) {
            label.set_css_classes(&["monospace", &format!("marked-line-{}", line_num)]);
        } else {
            label.set_css_classes(&["monospace"]);
        }
        content_box.append(&label);
    }
}
