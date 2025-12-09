mod cache;
mod error;
mod file_loader;
mod file_source;
mod remote_loader;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clap::Parser;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Adjustment, Application, ApplicationWindow, Label, Orientation, PolicyType, ScrolledWindow,
    Box as GtkBox, Scrollbar,
};

use file_loader::MappedFile;
use file_source::FileSource;
use remote_loader::RemoteFile;

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

    let app = Application::builder()
        .application_id("com.github.pog")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let file_source_clone = file_source.clone();

    app.connect_activate(move |app| {
        build_ui(app, file_source_clone.clone());
    });

    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &Application, file_source: Arc<dyn FileSource>) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title(&format!("pog - {}", file_source.display_name()))
        .default_width(1200)
        .default_height(800)
        .build();

    let total_lines = file_source.line_count();

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
                        populate_lines(&content_box_response, &lines);
                        *current_line_response.borrow_mut() = start;
                    }
                }
                FileResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
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

fn populate_lines(content_box: &GtkBox, lines: &[(usize, String)]) {
    // Clear
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    // Add lines
    for (line_num, text) in lines {
        let label = Label::new(Some(&format!("{:8} │ {}", line_num + 1, text)));
        label.set_halign(gtk4::Align::Start);
        label.set_selectable(true);
        label.set_css_classes(&["monospace"]);
        content_box.append(&label);
    }
}
