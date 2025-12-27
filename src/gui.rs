// Old Qt GUI removed. The GTK4 GUI implementation is in `src/gui_gtk.rs`.
// This file is kept as a placeholder for compatibility.

pub fn run(_config_path: &str) {
    eprintln!("Qt GUI removed. Use GTK4 GUI (build with --features gui).");
}
