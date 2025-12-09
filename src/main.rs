mod file_loader;

use clap::Parser;
use file_loader::MappedFile;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Label, ListBox, ListBoxRow, PolicyType, ScrolledWindow,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Parser)]
#[command(name = "pog")]
#[command(about = "A fast log file viewer")]
struct Args {
    file: PathBuf,
}

const VISIBLE_LINES: usize = 100;
const BUFFER_LINES: usize = 50;

fn main() -> glib::ExitCode {
    let args = Args::parse();

    let file_path = args.file.clone();
    let mapped_file = match MappedFile::open(&file_path) {
        Ok(f) => Rc::new(f),
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            std::process::exit(1);
        }
    };

    let app = Application::builder()
        .application_id("com.github.pog")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let file_path_clone = file_path.clone();
    let mapped_file_clone = mapped_file.clone();

    app.connect_activate(move |app| {
        build_ui(app, &file_path_clone, mapped_file_clone.clone());
    });

    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &Application, file_path: &PathBuf, mapped_file: Rc<MappedFile>) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title(&format!("pog - {}", file_path.display()))
        .default_width(1200)
        .default_height(800)
        .build();

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::None);

    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&list_box)
        .build();

    let total_lines = mapped_file.line_count();
    let current_start: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let current_end: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    populate_lines(
        &list_box,
        &mapped_file,
        0,
        VISIBLE_LINES.min(total_lines),
        &current_start,
        &current_end,
    );

    let vadj = scrolled_window.vadjustment();
    let mapped_file_scroll = mapped_file.clone();
    let list_box_scroll = list_box.clone();
    let current_start_scroll = current_start.clone();
    let current_end_scroll = current_end.clone();

    vadj.connect_value_changed(move |adj| {
        let value = adj.value();
        let upper = adj.upper();
        let page_size = adj.page_size();

        if upper <= page_size {
            return;
        }

        let scroll_fraction = value / (upper - page_size);
        let target_line = (scroll_fraction * (total_lines as f64)) as usize;
        let target_start = target_line.saturating_sub(BUFFER_LINES);

        let cs = *current_start_scroll.borrow();
        if target_start != cs && target_start.abs_diff(cs) > BUFFER_LINES / 2 {
            populate_lines(
                &list_box_scroll,
                &mapped_file_scroll,
                target_start,
                (target_start + VISIBLE_LINES + BUFFER_LINES * 2).min(total_lines),
                &current_start_scroll,
                &current_end_scroll,
            );
        }
    });

    window.set_child(Some(&scrolled_window));
    window.present();
}

fn populate_lines(
    list_box: &ListBox,
    mapped_file: &MappedFile,
    start: usize,
    end: usize,
    current_start: &Rc<RefCell<usize>>,
    current_end: &Rc<RefCell<usize>>,
) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }

    let lines = mapped_file.get_lines(start, end - start);
    for (line_num, text) in lines {
        let label = Label::new(Some(&format!("{:8} │ {}", line_num + 1, text)));
        label.set_halign(gtk4::Align::Start);
        label.set_selectable(true);
        label.set_css_classes(&["monospace"]);

        let row = ListBoxRow::new();
        row.set_child(Some(&label));
        list_box.append(&row);
    }

    *current_start.borrow_mut() = start;
    *current_end.borrow_mut() = end;
}
