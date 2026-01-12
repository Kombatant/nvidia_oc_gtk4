// Clean, redesigned GTK4 GUI

#[cfg(feature = "gui")]
pub mod imp {
    use libadwaita as adw;
    use adw::prelude::*;
    use gtk4::{Box as GtkBox, Orientation, Label, Button, Scale, Adjustment, TextView, ScrolledWindow, MessageDialog, MessageType, ButtonsType, Grid, ComboBoxText, CssProvider, LevelBar, DrawingArea, Separator, Revealer, Image, WindowControls, PackType, Settings, CenterBox, Entry};
    use gtk4::gdk;
    use gtk4::cairo;
    use gtk4::glib;

    fn build_command(gpu_index: &str, power: i32, freq: i32, mem: i32, min_clock: i32, max_clock: i32) -> String {
        let prog = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "zelos".to_string());
        format!("{} set --index {} --power-limit {} --freq-offset {} --mem-offset {} --min-clock {} --max-clock {}", prog, gpu_index, power, freq, mem, min_clock, max_clock)
    }

    fn list_nvidia_gpus() -> Vec<(String, String)> {
        if let Ok(out) = std::process::Command::new("nvidia-smi").arg("-L").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let mut v = Vec::new();
                for line in s.lines() {
                    if let Some(rest) = line.strip_prefix("GPU ") {
                        if let Some(colon_pos) = rest.find(':') {
                            let idx = rest[..colon_pos].trim().to_string();
                            let name_part = rest[colon_pos + 1..].trim();
                            let name = name_part.split('(').next().unwrap_or(name_part).trim().to_string();
                            let idx_clone = idx.clone();
                            v.push((idx, format!("GPU {}: {}", idx_clone, name)));
                        }
                    }
                }
                if !v.is_empty() {
                    return v;
                }
            }
        }
        vec![("0".to_string(), "GPU 0 (default)".to_string())]
    }

    fn show_message<P: gtk4::prelude::IsA<gtk4::Window> + Clone + 'static>(parent: Option<&P>, mtype: MessageType, buttons: ButtonsType, text: &str) {
        let parent_clone = parent.map(|p| p.clone().upcast::<gtk4::Window>());
        let dlg = MessageDialog::new(parent, gtk4::DialogFlags::MODAL, mtype, buttons, text);
        dlg.connect_response(move |d, _| {
            d.close();
            if let Some(ref p) = parent_clone {
                p.present();
            }
        });
        dlg.present();
    }

    pub fn run(_config_path: &str) {
        // Pre-parse existing systemd service ExecStart (if present) to pre-fill fields
        use std::os::unix::fs::PermissionsExt;
        let svc_path = "/etc/systemd/system/zelos.service";
        let mut svc_index: Option<String> = None;
        let mut svc_power: Option<i32> = None;
        let mut svc_freq: Option<i32> = None;
        let mut svc_mem: Option<i32> = None;
        let mut svc_min: Option<i32> = None;
        let mut svc_max: Option<i32> = None;
        if std::path::Path::new(svc_path).exists() {
            if let Ok(s) = std::fs::read_to_string(svc_path) {
                for line in s.lines() {
                    let line = line.trim();
                    if line.starts_with("ExecStart=") {
                        let mut exec = line.trim_start_matches("ExecStart=").trim().to_string();
                        if exec.starts_with('"') && exec.ends_with('"') && exec.len() > 1 {
                            exec = exec[1..exec.len() - 1].to_string();
                        }
                        let parts: Vec<&str> = exec.split_whitespace().collect();
                        let mut after_set = false;
                        let mut i = 0usize;
                        while i < parts.len() {
                            let p = parts[i];
                            if !after_set {
                                if p == "set" {
                                    after_set = true;
                                }
                                i += 1;
                                continue;
                            }
                            match p {
                                "--index" => {
                                    if i + 1 < parts.len() {
                                        svc_index = Some(parts[i + 1].to_string());
                                    }
                                    i += 2;
                                }
                                "--power-limit" => {
                                    if i + 1 < parts.len() {
                                        svc_power = parts[i + 1].parse().ok();
                                    }
                                    i += 2;
                                }
                                "--freq-offset" => {
                                    if i + 1 < parts.len() {
                                        svc_freq = parts[i + 1].parse().ok();
                                    }
                                    i += 2;
                                }
                                "--mem-offset" => {
                                    if i + 1 < parts.len() {
                                        svc_mem = parts[i + 1].parse().ok();
                                    }
                                    i += 2;
                                }
                                "--min-clock" => {
                                    if i + 1 < parts.len() {
                                        svc_min = parts[i + 1].parse().ok();
                                    }
                                    i += 2;
                                }
                                "--max-clock" => {
                                    if i + 1 < parts.len() {
                                        svc_max = parts[i + 1].parse().ok();
                                    }
                                    i += 2;
                                }
                                _ => {
                                    i += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Initialize GTK early so we can override problematic GtkSettings *before*
        // libadwaita initializes (prevents the warning).
        let _ = gtk4::init();
        if let Some(settings) = Settings::default() {
            use gtk4::glib::ObjectExt;
            settings.set_property("gtk-application-prefer-dark-theme", false);
        }

        // Ensure Adwaita styling is initialized.
        let _ = adw::init();

        // Follow the OS light/dark preference (libadwaita default behavior).
        // Explicitly setting Default avoids forcing a scheme.
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

        let app = adw::Application::new(Some("org.github.kombatant.zelos"), Default::default());

        app.connect_activate(move |app| {
            let window = adw::ApplicationWindow::new(app);
            // Explicitly set the window icon name so desktop environments
            // (KDE, GNOME, etc.) can match the running window to the
            // installed `.desktop` file and display the correct icon.
            window.set_icon_name(Some("zelos"));
            window.set_title(Some("Zelos"));
            // Matches the mock: maximize is disabled.
            window.set_resizable(false);

            // Note: removed elevated-action CSS provider — no red border shown.
            // Let GTK size the window to the content automatically

            // Performance tab (redesigned)
            let perf_box = GtkBox::new(Orientation::Vertical, 14);
            perf_box.set_margin_top(16);
            perf_box.set_margin_bottom(16);
            perf_box.set_margin_start(16);
            perf_box.set_margin_end(16);

            // GPU selector (reused inside the device card)
            let gpu_combo = ComboBoxText::new();
            gpu_combo.set_hexpand(true);
            for (id, label) in list_nvidia_gpus() {
                gpu_combo.append(Some(&id), &label);
            }
            if let Some(ref idx) = svc_index {
                gpu_combo.set_active_id(Some(idx));
            } else {
                gpu_combo.set_active_id(Some("0"));
            }

            // Try to read current GPU settings via NVML and prefer those over
            // values stored in the systemd service. If NVML is unavailable or
            // a query fails, fall back to the service values and then to
            // sensible defaults.
            let mut current_power: Option<i32> = None; // milliwatts
            let mut current_freq: Option<i32> = None; // MHz
            let mut current_mem: Option<i32> = None; // MHz
            let mut nvml_available = true;
            let gpu_index_num: u32 = svc_index.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
            match nvml_wrapper::Nvml::init() {
                Ok(nvml) => {
                    match nvml.device_by_index(gpu_index_num) {
                        Ok(device) => {
                            if let Ok(limit) = device.enforced_power_limit() {
                                current_power = Some(limit as i32);
                            }
                            if let Ok(freq) = device.gpc_clock_vf_offset() {
                                current_freq = Some(freq);
                            }
                            if let Ok(mem) = device.mem_clock_vf_offset() {
                                current_mem = Some(mem);
                            }
                        }
                        Err(_) => {
                            nvml_available = false;
                        }
                    }
                }
                Err(_) => {
                    nvml_available = false;
                }
            }
            if !nvml_available {
                show_message(Some(&window), MessageType::Warning, ButtonsType::Ok, "Warning: could not query NVML — using service values or defaults.");
            }

            // --- Device selection section ---
            let device_section_title = Label::new(Some("Device Selection"));
            device_section_title.set_halign(gtk4::Align::Start);
            device_section_title.set_css_classes(&["perf-section-title"]);
            perf_box.append(&device_section_title);

            let device_card = GtkBox::new(Orientation::Horizontal, 12);
            device_card.set_css_classes(&["card", "perf-card"]);
            device_card.set_hexpand(true);

            let device_left = GtkBox::new(Orientation::Horizontal, 12);
            device_left.set_hexpand(true);
            device_left.set_valign(gtk4::Align::Center);

            let device_badge = GtkBox::new(Orientation::Vertical, 0);
            device_badge.set_css_classes(&["perf-icon-badge"]);
            device_badge.set_valign(gtk4::Align::Center);
            device_badge.set_vexpand(false);
            let device_icon = Image::from_icon_name("video-display-symbolic");
            device_icon.set_pixel_size(18);
            device_icon.set_halign(gtk4::Align::Center);
            device_icon.set_valign(gtk4::Align::Center);
            device_badge.append(&device_icon);

            let device_texts = GtkBox::new(Orientation::Vertical, 2);
            device_texts.set_valign(gtk4::Align::Center);
            let device_title = Label::new(Some("Target GPU"));
            device_title.set_halign(gtk4::Align::Start);
            device_title.set_valign(gtk4::Align::Center);
            device_title.set_css_classes(&["perf-card-title"]);
            let device_sub = Label::new(Some("Select the card to overclock"));
            device_sub.set_halign(gtk4::Align::Start);
            device_sub.set_valign(gtk4::Align::Center);
            device_sub.set_css_classes(&["perf-card-subtitle"]);
            device_texts.append(&device_title);
            device_texts.append(&device_sub);

            device_left.append(&device_badge);
            device_left.append(&device_texts);

            gpu_combo.set_hexpand(false);
            gpu_combo.set_halign(gtk4::Align::End);
            gpu_combo.set_valign(gtk4::Align::Center);
            gpu_combo.set_css_classes(&["perf-combo"]);

            device_card.append(&device_left);
            device_card.append(&gpu_combo);
            perf_box.append(&device_card);

            // --- Power configuration section ---
            let power_section_title = Label::new(Some("Power Configuration"));
            power_section_title.set_halign(gtk4::Align::Start);
            power_section_title.set_css_classes(&["perf-section-title"]);
            power_section_title.set_margin_top(4);
            perf_box.append(&power_section_title);

            let power_initial = match current_power {
                Some(p) => p as f64 / 1000.0,
                None => svc_power.unwrap_or(400_000) as f64 / 1000.0,
            };
            // Match the redesign (common 450W max on many cards)
            let power_adj = Adjustment::new(power_initial, 0.0, 450.0, 0.1, 1.0, 0.0);
            let power_scale = Scale::new(Orientation::Horizontal, Some(&power_adj));
            power_scale.set_hexpand(true);
            power_scale.set_draw_value(false);

            let power_card = GtkBox::new(Orientation::Vertical, 10);
            power_card.set_css_classes(&["card", "perf-card"]);
            power_card.set_hexpand(true);

            let power_top = GtkBox::new(Orientation::Horizontal, 12);
            power_top.set_hexpand(true);
            power_top.set_valign(gtk4::Align::Center);

            let power_left = GtkBox::new(Orientation::Horizontal, 12);
            power_left.set_hexpand(true);
            power_left.set_valign(gtk4::Align::Center);

            let power_badge = GtkBox::new(Orientation::Vertical, 0);
            power_badge.set_css_classes(&["perf-icon-badge"]);
            power_badge.set_valign(gtk4::Align::Center);
            power_badge.set_vexpand(false);
            let power_icon = Image::from_icon_name("battery-level-100-symbolic");
            power_icon.set_pixel_size(18);
            power_icon.set_halign(gtk4::Align::Center);
            power_icon.set_valign(gtk4::Align::Center);
            power_badge.append(&power_icon);

            let power_texts = GtkBox::new(Orientation::Vertical, 2);
            power_texts.set_valign(gtk4::Align::Center);
            let power_title = Label::new(Some("Power Limit (W)"));
            power_title.set_halign(gtk4::Align::Start);
            power_title.set_valign(gtk4::Align::Center);
            power_title.set_css_classes(&["perf-card-title"]);
            let power_sub = Label::new(Some(&format!("Max {}W", power_adj.upper().round() as i32)));
            power_sub.set_halign(gtk4::Align::Start);
            power_sub.set_valign(gtk4::Align::Center);
            power_sub.set_css_classes(&["perf-card-subtitle"]);
            power_texts.append(&power_title);
            power_texts.append(&power_sub);

            power_left.append(&power_badge);
            power_left.append(&power_texts);

            let power_value_lbl = Label::new(Some(&format!("{}", power_adj.value().round() as i32)));
            power_value_lbl.set_halign(gtk4::Align::End);
            power_value_lbl.set_valign(gtk4::Align::Center);
            power_value_lbl.set_css_classes(&["perf-row-value"]);

            power_top.append(&power_left);
            power_top.append(&power_value_lbl);
            power_card.append(&power_top);
            power_card.append(&power_scale);
            perf_box.append(&power_card);

            let freq_initial = current_freq.unwrap_or(svc_freq.unwrap_or(0));
            let freq_adj = Adjustment::new(freq_initial as f64, -2000.0, 2000.0, 1.0, 10.0, 0.0);
            let mem_initial = current_mem.unwrap_or(svc_mem.unwrap_or(0));
            let mem_adj = Adjustment::new(mem_initial as f64, -20000.0, 20000.0, 1.0, 10.0, 0.0);

            let min_initial = svc_min.unwrap_or(0);
            let min_adj = Adjustment::new(min_initial as f64, 0.0, 5000.0, 1.0, 10.0, 0.0);
            let max_initial = svc_max.unwrap_or(3800);
            let max_adj = Adjustment::new(max_initial as f64, 0.0, 5000.0, 1.0, 10.0, 0.0);

            // Rows card (GPU freq/mem/min/max) with steppers
            let rows_card = GtkBox::new(Orientation::Vertical, 0);
            rows_card.set_css_classes(&["card", "perf-card"]);
            rows_card.set_hexpand(true);

            let build_step_row = |icon_name: &str, title: &str, adj: &Adjustment, step: f64| -> (GtkBox, Entry) {
                let row = GtkBox::new(Orientation::Horizontal, 12);
                row.set_css_classes(&["perf-row"]);
                row.set_hexpand(true);
                row.set_valign(gtk4::Align::Center);

                let icon = Image::from_icon_name(icon_name);
                icon.set_pixel_size(18);
                icon.set_halign(gtk4::Align::Center);
                icon.set_valign(gtk4::Align::Center);

                // Use CenterBox so the icon is truly centered within the
                // fixed-size badge area (GtkBox would top-pack the child).
                let badge = CenterBox::new();
                badge.set_css_classes(&["perf-icon-badge"]);
                badge.set_halign(gtk4::Align::Center);
                badge.set_valign(gtk4::Align::Center);
                badge.set_center_widget(Some(&icon));

                let title_lbl = Label::new(Some(title));
                title_lbl.set_halign(gtk4::Align::Start);
                title_lbl.set_valign(gtk4::Align::Center);
                title_lbl.set_hexpand(true);
                title_lbl.set_css_classes(&["perf-row-title"]);

                // Replace the static value label with an editable numeric entry
                let value_entry = Entry::new();
                value_entry.set_text(&format!("{}", adj.value().round() as i32));
                // Right-align the entry text so it matches the previous label layout
                libadwaita::prelude::EntryExt::set_alignment(&value_entry, 1.0);
                value_entry.set_valign(gtk4::Align::Center);
                // Keep the value column compact: enough for ~5 digits (+ optional minus)
                value_entry.set_width_chars(6);
                value_entry.set_max_width_chars(6);
                value_entry.set_hexpand(false);
                value_entry.set_css_classes(&["perf-row-value"]);

                let minus = Button::with_label("−");
                minus.set_css_classes(&["perf-stepper"]);
                minus.set_valign(gtk4::Align::Center);
                let plus = Button::with_label("+");
                plus.set_css_classes(&["perf-stepper"]);
                plus.set_valign(gtk4::Align::Center);

                let adj_minus = adj.clone();
                minus.connect_clicked(move |_| {
                    let v = (adj_minus.value() - step).clamp(adj_minus.lower(), adj_minus.upper());
                    adj_minus.set_value(v);
                });
                let adj_plus = adj.clone();
                plus.connect_clicked(move |_| {
                    let v = (adj_plus.value() + step).clamp(adj_plus.lower(), adj_plus.upper());
                    adj_plus.set_value(v);
                });

                let value_entry_cl = value_entry.clone();
                let adj_for_entry = adj.clone();
                adj.connect_value_changed(move |_| {
                    value_entry_cl.set_text(&format!("{}", adj_for_entry.value().round() as i32));
                });

                // When the user activates the entry (presses Enter), parse and apply to the adjustment.
                let adj_for_entry2 = adj.clone();
                value_entry.connect_activate(move |e| {
                    if let Ok(v) = e.text().as_str().trim().parse::<i32>() {
                        let v = (v as f64).clamp(adj_for_entry2.lower(), adj_for_entry2.upper());
                        adj_for_entry2.set_value(v);
                    } else {
                        // Reset to the current adjustment value on parse failure
                        e.set_text(&format!("{}", adj_for_entry2.value().round() as i32));
                    }
                });

                row.append(&badge);
                row.append(&title_lbl);
                row.append(&value_entry);
                row.append(&minus);
                row.append(&plus);
                (row, value_entry)
            };

            let (freq_row, _freq_val_lbl) = build_step_row("speedometer-symbolic", "GPU Freq Offset (MHz)", &freq_adj, 1.0);
            rows_card.append(&freq_row);
            rows_card.append(&Separator::new(Orientation::Horizontal));

            let (mem_row, _mem_val_lbl) = build_step_row("media-floppy-symbolic", "Memory Offset (MHz)", &mem_adj, 1.0);
            rows_card.append(&mem_row);
            rows_card.append(&Separator::new(Orientation::Horizontal));

            let (min_row, _min_val_lbl) = build_step_row("go-down-symbolic", "Min Clock (MHz)", &min_adj, 1.0);
            rows_card.append(&min_row);
            rows_card.append(&Separator::new(Orientation::Horizontal));

            let (max_row, _max_val_lbl) = build_step_row("go-up-symbolic", "Max Clock (MHz)", &max_adj, 1.0);
            rows_card.append(&max_row);

            perf_box.append(&rows_card);

            // Command preview card (collapsible)
            let preview_card = GtkBox::new(Orientation::Vertical, 10);
            preview_card.set_css_classes(&["card", "perf-card"]);
            preview_card.set_hexpand(true);

            let preview_header = GtkBox::new(Orientation::Horizontal, 12);
            preview_header.set_hexpand(true);
            preview_header.set_valign(gtk4::Align::Center);

            let preview_left = GtkBox::new(Orientation::Horizontal, 12);
            preview_left.set_hexpand(true);
            preview_left.set_valign(gtk4::Align::Center);

            let preview_badge = GtkBox::new(Orientation::Vertical, 0);
            preview_badge.set_css_classes(&["perf-icon-badge"]);
            preview_badge.set_valign(gtk4::Align::Center);
            preview_badge.set_vexpand(false);
            let preview_icon = Image::from_icon_name("utilities-terminal-symbolic");
            preview_icon.set_pixel_size(18);
            preview_icon.set_halign(gtk4::Align::Center);
            preview_icon.set_valign(gtk4::Align::Center);
            preview_badge.append(&preview_icon);

            let preview_texts = GtkBox::new(Orientation::Vertical, 2);
            preview_texts.set_valign(gtk4::Align::Center);
            let preview_title = Label::new(Some("Command Preview"));
            preview_title.set_halign(gtk4::Align::Start);
            preview_title.set_valign(gtk4::Align::Center);
            preview_title.set_css_classes(&["perf-card-title"]);
            let preview_sub = Label::new(Some("View generated CLI command"));
            preview_sub.set_halign(gtk4::Align::Start);
            preview_sub.set_valign(gtk4::Align::Center);
            preview_sub.set_css_classes(&["perf-card-subtitle"]);
            preview_texts.append(&preview_title);
            preview_texts.append(&preview_sub);

            preview_left.append(&preview_badge);
            preview_left.append(&preview_texts);

            let preview_toggle = Button::with_label("▴");
            preview_toggle.set_css_classes(&["perf-disclosure"]);
            preview_toggle.set_halign(gtk4::Align::End);
            preview_toggle.set_valign(gtk4::Align::Center);

            preview_header.append(&preview_left);
            preview_header.append(&preview_toggle);
            preview_card.append(&preview_header);

            let scrolled = ScrolledWindow::new();
            // Make preview tall enough for ~3 lines of text
            scrolled.set_min_content_height(84);
            scrolled.set_hexpand(true);
            scrolled.set_vexpand(false);
            let preview = TextView::new();
            preview.set_editable(false);
            preview.set_monospace(true);
            preview.set_pixels_above_lines(2);
            preview.set_pixels_below_lines(2);
            preview.set_wrap_mode(gtk4::WrapMode::WordChar);
            scrolled.set_child(Some(&preview));

            let preview_revealer = Revealer::new();
            preview_revealer.set_reveal_child(true);
            preview_revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
            preview_revealer.set_child(Some(&scrolled));
            preview_card.append(&preview_revealer);
            perf_box.append(&preview_card);

            // Bottom action bar
            let actions = GtkBox::new(Orientation::Horizontal, 12);
            actions.set_hexpand(true);
            actions.set_margin_top(2);

            let service_btn = Button::with_label(if std::path::Path::new(svc_path).exists() { "Update Service" } else { "Create Service" });
            // Match the mock: secondary action with red/destructive emphasis.
            service_btn.set_css_classes(&["destructive-action", "perf-action-secondary"]);

            let apply = Button::with_label("Apply Settings");
            apply.set_css_classes(&["suggested-action", "perf-action-primary"]);

            let spacer = GtkBox::new(Orientation::Horizontal, 0);
            spacer.set_hexpand(true);

            actions.append(&service_btn);
            actions.append(&spacer);
            actions.append(&apply);
            perf_box.append(&actions);

            // Power limit value label
            {
                let power_value_lbl = power_value_lbl.clone();
                let power_adj_for_lbl = power_adj.clone();
                power_adj.connect_value_changed(move |_| {
                    power_value_lbl.set_text(&format!("{}", power_adj_for_lbl.value().round() as i32));
                });
            }

            // Command preview disclosure
            {
                use std::cell::Cell;
                let expanded = Cell::new(true);
                let revealer = preview_revealer.clone();
                let toggle_btn = preview_toggle.clone();
                preview_toggle.connect_clicked(move |_| {
                    let now = !expanded.get();
                    expanded.set(now);
                    revealer.set_reveal_child(now);
                    toggle_btn.set_label(if now { "▴" } else { "▾" });
                });
            }

            // Metrics tab -------------------------------------------------
            let metrics_box = GtkBox::new(Orientation::Vertical, 12);
            metrics_box.set_margin_top(12);
            metrics_box.set_margin_bottom(12);
            metrics_box.set_margin_start(12);
            metrics_box.set_margin_end(12);

            // Professional layout:
            // - Top: 2x2 grid of cards
            // - Bottom: two full-width history charts (memory, then core)
            let metrics_layout = GtkBox::new(Orientation::Vertical, 12);
            metrics_layout.set_hexpand(true);
            metrics_layout.set_vexpand(true);

            let top_grid = Grid::new();
            top_grid.set_column_spacing(12);
            top_grid.set_row_spacing(12);
            top_grid.set_hexpand(true);
            top_grid.set_column_homogeneous(true);
            top_grid.set_row_homogeneous(false);

            // Shared gauge state for DrawingArea widgets
            use std::cell::RefCell;
            use std::collections::VecDeque;
            use std::rc::Rc;
            use std::time::{Duration, Instant};
            #[derive(Clone, Copy, Default)]
            struct GaugeState {
                vram_used_mib: f64,
                vram_total_mib: f64,
                fan_pct: f64,
            }
            let gauge_state = Rc::new(RefCell::new(GaugeState::default()));

            // History charts (kept in MHz). Time is seconds since chart start.
            struct TimeSeriesState {
                points: VecDeque<(f64, f64)>,
            }
            let chart_start = Instant::now();

            let core_clock_history: Rc<RefCell<TimeSeriesState>> = Rc::new(RefCell::new(TimeSeriesState {
                points: {
                    let mut v = VecDeque::with_capacity(300);
                    // Assume x=0s and y=0MHz when plotting begins.
                    v.push_back((0.0, 0.0));
                    v
                },
            }));

            let mem_clock_history: Rc<RefCell<TimeSeriesState>> = Rc::new(RefCell::new(TimeSeriesState {
                points: {
                    let mut v = VecDeque::with_capacity(300);
                    v.push_back((0.0, 0.0));
                    v
                },
            }));

            // --- Left: VRAM card (circular gauge) ---
            let vram_card = GtkBox::new(Orientation::Vertical, 10);
            vram_card.set_css_classes(&["card", "metrics-card"]);
            vram_card.set_hexpand(true);

            let vram_title = Label::new(Some("VRAM Usage"));
            vram_title.set_halign(gtk4::Align::Start);
            vram_title.set_css_classes(&["metrics-title"]);
            vram_card.append(&vram_title);

            let vram_gauge = DrawingArea::new();
            // Slightly smaller to better match the right-side stacked cards.
            vram_gauge.set_content_width(250);
            vram_gauge.set_content_height(200);
            vram_gauge.set_hexpand(true);
            vram_gauge.set_vexpand(true);

            let vram_center_used = Label::new(Some("0"));
            vram_center_used.set_css_classes(&["metrics-gauge-big"]);
            vram_center_used.set_halign(gtk4::Align::Center);
            let vram_center_total = Label::new(Some("/ 0 MiB"));
            vram_center_total.set_css_classes(&["metrics-gauge-small"]);
            vram_center_total.set_halign(gtk4::Align::Center);

            let vram_center_box = GtkBox::new(Orientation::Vertical, 0);
            vram_center_box.set_halign(gtk4::Align::Center);
            vram_center_box.set_valign(gtk4::Align::Center);
            vram_center_box.append(&vram_center_used);
            vram_center_box.append(&vram_center_total);

            let vram_overlay = gtk4::Overlay::new();
            vram_overlay.set_child(Some(&vram_gauge));
            vram_overlay.add_overlay(&vram_center_box);
            vram_overlay.set_hexpand(true);
            vram_overlay.set_vexpand(true);
            vram_card.append(&vram_overlay);

            // Draw the VRAM arc
            {
                let gauge_state = gauge_state.clone();
                vram_gauge.set_draw_func(move |_, cr, w, h| {
                    let st = *gauge_state.borrow();
                    let cx = (w as f64) / 2.0;
                    let cy = (h as f64) / 2.0;
                    let r = (w.min(h) as f64) * 0.38;
                    let thickness = (w.min(h) as f64) * 0.06;

                    let start = std::f64::consts::PI * 0.75;
                    let end = std::f64::consts::PI * 2.25;
                    let frac = if st.vram_total_mib > 0.0 {
                        (st.vram_used_mib / st.vram_total_mib).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let sweep = start + (end - start) * frac;

                    cr.set_line_width(thickness);
                    cr.set_line_cap(cairo::LineCap::Round);

                    // Track
                    cr.set_source_rgba(0.17, 0.17, 0.17, 1.0);
                    cr.arc(cx, cy, r, start, end);
                    let _ = cr.stroke();

                    // Fill (re-use existing accent color)
                    cr.set_source_rgba(0.18, 0.53, 1.0, 1.0);
                    cr.arc(cx, cy, r, start, sweep);
                    let _ = cr.stroke();
                });
            }

            // Top-left
            top_grid.attach(&vram_card, 0, 0, 1, 1);

            // --- Stats strip (goes into the chart header) ---
            let stat_core_value = Label::new(Some("N/A"));
            stat_core_value.set_halign(gtk4::Align::Start);
            stat_core_value.set_css_classes(&["metrics-stat-value"]);
            // Keep width stable as values update (prevents window width changes).
            stat_core_value.set_width_chars(10);

            let stat_mem_value = Label::new(Some("N/A"));
            stat_mem_value.set_halign(gtk4::Align::Start);
            stat_mem_value.set_css_classes(&["metrics-stat-value"]);
            stat_mem_value.set_width_chars(10);

            let stat_temp_value = Label::new(Some("N/A"));
            stat_temp_value.set_halign(gtk4::Align::Start);
            stat_temp_value.set_css_classes(&["metrics-stat-value"]);
            stat_temp_value.set_width_chars(6);

            let mk_stat_value = |value: &Label| {
                let b = GtkBox::new(Orientation::Vertical, 0);
                b.append(value);
                b
            };

            let stats_strip = GtkBox::new(Orientation::Horizontal, 18);
            stats_strip.set_halign(gtk4::Align::End);
            stats_strip.append(&mk_stat_value(&stat_core_value));

            let mem_stats_strip = GtkBox::new(Orientation::Horizontal, 18);
            mem_stats_strip.set_halign(gtk4::Align::End);
            mem_stats_strip.append(&mk_stat_value(&stat_mem_value));

            let temp_stats_strip = GtkBox::new(Orientation::Horizontal, 18);
            temp_stats_strip.set_halign(gtk4::Align::End);
            temp_stats_strip.append(&mk_stat_value(&stat_temp_value));

            // --- Right: GPU usage card ---
            let usage_card = GtkBox::new(Orientation::Vertical, 8);
            usage_card.set_css_classes(&["card", "metrics-card", "metrics-card-snug"]);
            usage_card.set_hexpand(true);

            let usage_title = Label::new(Some("GPU Usage"));
            usage_title.set_halign(gtk4::Align::Start);
            usage_title.set_css_classes(&["metrics-title"]);
            usage_card.append(&usage_title);

            // Keep the bar vertically centered inside the card by overlaying
            // the numeric value on top of the `LevelBar` and letting the bar
            // expand. This mirrors the VRAM gauge's centered value style.
            let usage_value = Label::new(Some("0%"));
            usage_value.set_halign(gtk4::Align::Center);
            usage_value.set_css_classes(&["metrics-value"]);
            usage_value.set_width_chars(4);

            let usage_bar_box = GtkBox::new(Orientation::Vertical, 0);
            usage_bar_box.set_hexpand(true);
            usage_bar_box.set_vexpand(true);
            usage_bar_box.set_valign(gtk4::Align::Center);

            let usage_bar = LevelBar::new();
            usage_bar.set_min_value(0.0);
            usage_bar.set_max_value(100.0);
            usage_bar.set_value(0.0);
            usage_bar.set_hexpand(true);
            usage_bar.set_css_classes(&["nvidia-progress"]);

            let usage_overlay = gtk4::Overlay::new();
            usage_overlay.set_child(Some(&usage_bar));
            let usage_center_box = GtkBox::new(Orientation::Vertical, 0);
            usage_center_box.set_halign(gtk4::Align::Center);
            usage_center_box.set_valign(gtk4::Align::Center);
            usage_center_box.append(&usage_value);
            usage_overlay.add_overlay(&usage_center_box);

            usage_bar_box.append(&usage_overlay);
            usage_card.append(&usage_bar_box);

            // --- Right: power usage card ---
            let power_card = GtkBox::new(Orientation::Vertical, 8);
            power_card.set_css_classes(&["card", "metrics-card", "metrics-card-snug"]);
            power_card.set_hexpand(true);

            let power_title = Label::new(Some("Power Usage"));
            power_title.set_halign(gtk4::Align::Start);
            power_title.set_css_classes(&["metrics-title"]);
            power_card.append(&power_title);

            // Vertically center the power bar in the card like the GPU usage
            // bar, with the numeric value shown below.
            let power_value = Label::new(Some("N/A"));
            power_value.set_halign(gtk4::Align::Center);
            power_value.set_css_classes(&["metrics-value"]);
            power_value.set_width_chars(18);

            let power_bar_box = GtkBox::new(Orientation::Vertical, 0);
            power_bar_box.set_hexpand(true);
            power_bar_box.set_vexpand(true);
            power_bar_box.set_valign(gtk4::Align::Center);

            let power_bar = LevelBar::new();
            power_bar.set_min_value(0.0);
            power_bar.set_max_value(1.0);
            power_bar.set_value(0.0);
            power_bar.set_hexpand(true);
            power_bar.set_css_classes(&["nvidia-progress"]);

            let power_overlay = gtk4::Overlay::new();
            power_overlay.set_child(Some(&power_bar));
            let power_center_box = GtkBox::new(Orientation::Vertical, 0);
            power_center_box.set_halign(gtk4::Align::Center);
            power_center_box.set_valign(gtk4::Align::Center);
            power_center_box.append(&power_value);
            power_overlay.add_overlay(&power_center_box);

            power_bar_box.append(&power_overlay);
            power_card.append(&power_bar_box);

            // Top grid (2x2):
            // (0,0) VRAM, (1,0) Fan, (0,1) Usage, (1,1) Power

            // --- Right: fan speed card (small gauge) ---
            let fan_card = GtkBox::new(Orientation::Vertical, 10);
            fan_card.set_css_classes(&["card", "metrics-card"]);
            fan_card.set_hexpand(true);

            let fan_title = Label::new(Some("Fan Speed"));
            fan_title.set_halign(gtk4::Align::Start);
            fan_title.set_css_classes(&["metrics-title"]);

            let fan_header = GtkBox::new(Orientation::Horizontal, 12);
            fan_header.set_hexpand(true);
            fan_header.append(&fan_title);
            let fan_header_spacer = GtkBox::new(Orientation::Horizontal, 0);
            fan_header_spacer.set_hexpand(true);
            fan_header.append(&fan_header_spacer);
            fan_card.append(&fan_header);

            let fan_gauge = DrawingArea::new();
            fan_gauge.set_content_width(250);
            fan_gauge.set_content_height(160);
            fan_gauge.set_hexpand(true);
            fan_gauge.set_vexpand(true);

            let fan_center_pct = Label::new(Some("N/A"));
            fan_center_pct.set_halign(gtk4::Align::Center);
            fan_center_pct.set_css_classes(&["metrics-gauge-mid"]);
            fan_center_pct.set_width_chars(6);

            let fan_center_rpm = Label::new(Some("N/A"));
            fan_center_rpm.set_halign(gtk4::Align::Center);
            fan_center_rpm.set_css_classes(&["metrics-gauge-small"]);
            fan_center_rpm.set_width_chars(10);

            let fan_center_box = GtkBox::new(Orientation::Vertical, 0);
            fan_center_box.set_halign(gtk4::Align::Center);
            fan_center_box.set_valign(gtk4::Align::Center);
            fan_center_box.append(&fan_center_pct);
            fan_center_box.append(&fan_center_rpm);

            let fan_overlay = gtk4::Overlay::new();
            fan_overlay.set_child(Some(&fan_gauge));
            fan_overlay.add_overlay(&fan_center_box);
            fan_overlay.set_hexpand(true);
            fan_overlay.set_vexpand(true);
            fan_card.append(&fan_overlay);

            // Bottom-right: Temperature
            let fan_footer = GtkBox::new(Orientation::Horizontal, 0);
            fan_footer.set_hexpand(true);
            let fan_footer_spacer = GtkBox::new(Orientation::Horizontal, 0);
            fan_footer_spacer.set_hexpand(true);
            fan_footer.append(&fan_footer_spacer);
            fan_footer.append(&temp_stats_strip);
            fan_card.append(&fan_footer);

            {
                let gauge_state = gauge_state.clone();
                fan_gauge.set_draw_func(move |_, cr, w, h| {
                    let st = *gauge_state.borrow();
                    let cx = (w as f64) / 2.0;
                    let cy = (h as f64) / 2.0;
                    let r = (w.min(h) as f64) * 0.38;
                    let thickness = (w.min(h) as f64) * 0.08;
                    let start = std::f64::consts::PI * 0.75;
                    let end = std::f64::consts::PI * 2.25;
                    let frac = (st.fan_pct / 100.0).clamp(0.0, 1.0);
                    let sweep = start + (end - start) * frac;

                    cr.set_line_width(thickness);
                    cr.set_line_cap(cairo::LineCap::Round);
                    cr.set_source_rgba(0.17, 0.17, 0.17, 1.0);
                    cr.arc(cx, cy, r, start, end);
                    let _ = cr.stroke();
                    cr.set_source_rgba(0.18, 0.53, 1.0, 1.0);
                    cr.arc(cx, cy, r, start, sweep);
                    let _ = cr.stroke();
                });
            }

            // Top-right
            fan_card.set_vexpand(true);
            fan_card.set_valign(gtk4::Align::Fill);
            top_grid.attach(&fan_card, 1, 0, 1, 1);

            // Bottom-left (top grid)
            usage_card.set_vexpand(true);
            usage_card.set_valign(gtk4::Align::Fill);
            top_grid.attach(&usage_card, 0, 1, 1, 1);

            // Bottom-right (top grid)
            power_card.set_vexpand(true);
            power_card.set_valign(gtk4::Align::Fill);
            top_grid.attach(&power_card, 1, 1, 1, 1);

            // --- Bottom: Memory clock history line chart ---
            let mem_chart_card = GtkBox::new(Orientation::Vertical, 10);
            mem_chart_card.set_css_classes(&["card", "metrics-card"]);
            mem_chart_card.set_hexpand(true);
            mem_chart_card.set_vexpand(true);
            mem_chart_card.set_valign(gtk4::Align::Fill);

            let mem_chart_title = Label::new(Some("Memory Clock History"));
            mem_chart_title.set_halign(gtk4::Align::Start);
            mem_chart_title.set_css_classes(&["metrics-title"]);

            let mem_chart_header = GtkBox::new(Orientation::Horizontal, 12);
            mem_chart_header.set_hexpand(true);
            mem_chart_header.append(&mem_chart_title);
            let mem_header_spacer = GtkBox::new(Orientation::Horizontal, 0);
            mem_header_spacer.set_hexpand(true);
            mem_chart_header.append(&mem_header_spacer);
            mem_chart_card.append(&mem_chart_header);

            let mem_chart = DrawingArea::new();
            mem_chart.set_hexpand(true);
            mem_chart.set_vexpand(true);
            mem_chart.set_content_height(190);
            // Keep the overall window from expanding; allow the chart to shrink.
            mem_chart.set_content_width(250);
            mem_chart_card.append(&mem_chart);

            // Bottom-right: Memory clock value
            let mem_chart_footer = GtkBox::new(Orientation::Horizontal, 0);
            mem_chart_footer.set_hexpand(true);
            let mem_footer_spacer = GtkBox::new(Orientation::Horizontal, 0);
            mem_footer_spacer.set_hexpand(true);
            mem_chart_footer.append(&mem_footer_spacer);
            mem_chart_footer.append(&mem_stats_strip);
            mem_chart_card.append(&mem_chart_footer);

            {
                let history = mem_clock_history.clone();
                let start = chart_start;
                mem_chart.set_draw_func(move |_, cr, w, h| {
                    let w = w as f64;
                    let h = h as f64;
                    let pad = 12.0;
                    let label_pad = 16.0;
                    let plot_w = (w - 2.0 * pad).max(1.0);
                    let plot_h = (h - 2.0 * pad).max(1.0);

                    // X behavior:
                    // - Fill left->right from x=0s.
                    // - Start scrolling only after reaching 95% of the chart width.
                    const WINDOW_SECS: f64 = 60.0;
                    let fill_span = WINDOW_SECS * 0.95;
                    let usable_w = plot_w * 0.95;
                    let now_t = start.elapsed().as_secs_f64();
                    let start_t = if now_t > fill_span { now_t - fill_span } else { 0.0 };

                    // Axis label (Y only)
                    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                    cr.set_font_size(12.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.55);

                    // Y-axis label (rotated, centered on left)
                    cr.save().ok();
                    cr.translate(10.0, h / 2.0);
                    cr.rotate(-std::f64::consts::FRAC_PI_2);
                    let y_label = "Memory Clock (MHz)";
                    let (yext_w, yext_xb) = match cr.text_extents(y_label) {
                        Ok(ext) => (ext.width(), ext.x_bearing()),
                        Err(_) => (0.0, 0.0),
                    };
                    cr.move_to(-yext_w / 2.0 - yext_xb, -label_pad);
                    let _ = cr.show_text(y_label);
                    cr.restore().ok();

                    let st = history.borrow();
                    if st.points.is_empty() {
                        return;
                    }

                    // Y axis anchored at 0 MHz.
                    let min_v: f64 = 0.0;
                    let mut max_v: f64 = 0.0;
                    let end_t = start_t + fill_span;
                    for &(_t, v) in st.points.iter() {
                        if v.is_finite() {
                            max_v = max_v.max(v);
                        }
                    }
                    if max_v <= 0.0 {
                        max_v = 1.0;
                    } else {
                        max_v += max_v * 0.08;
                    }

                    let denom = (max_v - min_v).max(1e-9_f64);

                    // Grid + Y ticks
                    cr.set_line_width(1.0);
                    for i in 0..=4 {
                        let t = (i as f64) / 4.0;
                        let y = pad + plot_h * t;

                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
                        cr.move_to(pad, y);
                        cr.line_to(pad + plot_w, y);
                        let _ = cr.stroke();

                        let value = max_v - (max_v - min_v) * t;
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.45);
                        cr.set_font_size(11.0);
                        cr.move_to(pad + 4.0, y - 2.0);
                        let _ = cr.show_text(&format!("{:.0}", value));
                    }

                    // Axes at (0,0)
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
                    cr.set_line_width(1.0);
                    cr.move_to(pad, pad);
                    cr.line_to(pad, pad + plot_h);
                    let y0 = pad + plot_h;
                    cr.move_to(pad, y0);
                    cr.line_to(pad + usable_w, y0);
                    let _ = cr.stroke();

                    // Line
                    cr.set_line_width(2.0);
                    cr.set_line_cap(cairo::LineCap::Round);
                    cr.set_line_join(cairo::LineJoin::Round);
                    cr.set_source_rgba(0.18, 0.53, 1.0, 1.0);

                    let last_y = st.points.back().map(|p| p.1).unwrap_or(0.0);
                    let mut started = false;
                    for &(t, v) in st.points.iter() {
                        if t < start_t {
                            continue;
                        }
                        if t > end_t {
                            break;
                        }
                        let x = pad + ((t - start_t) / fill_span).clamp(0.0, 1.0) * usable_w;
                        let y = pad + (1.0 - ((v - min_v) / denom).clamp(0.0, 1.0)) * plot_h;
                        if !started {
                            cr.move_to(x, y);
                            started = true;
                        } else {
                            cr.line_to(x, y);
                        }
                    }

                    let t_now = now_t.min(end_t);
                    if t_now >= start_t {
                        let x = pad + ((t_now - start_t) / fill_span).clamp(0.0, 1.0) * usable_w;
                        let y = pad + (1.0 - ((last_y - min_v) / denom).clamp(0.0, 1.0)) * plot_h;
                        if !started {
                            cr.move_to(x, y);
                        } else {
                            cr.line_to(x, y);
                        }
                    }
                    let _ = cr.stroke();
                });
            }

            // --- Bottom: Core clock history line chart ---
            let core_chart_card = GtkBox::new(Orientation::Vertical, 10);
            core_chart_card.set_css_classes(&["card", "metrics-card"]);
            core_chart_card.set_hexpand(true);
            core_chart_card.set_vexpand(true);
            core_chart_card.set_valign(gtk4::Align::Fill);

            let core_chart_title = Label::new(Some("Core Clock History"));
            core_chart_title.set_halign(gtk4::Align::Start);
            core_chart_title.set_css_classes(&["metrics-title"]);

            let chart_header = GtkBox::new(Orientation::Horizontal, 12);
            chart_header.set_hexpand(true);
            chart_header.append(&core_chart_title);
            let header_spacer = GtkBox::new(Orientation::Horizontal, 0);
            header_spacer.set_hexpand(true);
            chart_header.append(&header_spacer);
            core_chart_card.append(&chart_header);

            let core_chart = DrawingArea::new();
            core_chart.set_hexpand(true);
            core_chart.set_vexpand(true);
            core_chart.set_content_height(190);
            // Keep the overall window from expanding; allow the chart to shrink.
            core_chart.set_content_width(250);
            core_chart_card.append(&core_chart);

            // Bottom-right: Core clock value
            let core_chart_footer = GtkBox::new(Orientation::Horizontal, 0);
            core_chart_footer.set_hexpand(true);
            let core_footer_spacer = GtkBox::new(Orientation::Horizontal, 0);
            core_footer_spacer.set_hexpand(true);
            core_chart_footer.append(&core_footer_spacer);
            core_chart_footer.append(&stats_strip);
            core_chart_card.append(&core_chart_footer);

            {
                let history = core_clock_history.clone();
                let start = chart_start;
                core_chart.set_draw_func(move |_, cr, w, h| {
                    let w = w as f64;
                    let h = h as f64;
                    let pad = 12.0;
                    let label_pad = 16.0;
                    let plot_w = (w - 2.0 * pad).max(1.0);
                    let plot_h = (h - 2.0 * pad).max(1.0);

                    // X behavior:
                    // - Fill left->right from x=0s.
                    // - Start scrolling only after reaching 95% of the chart width.
                    const WINDOW_SECS: f64 = 60.0;
                    let fill_span = WINDOW_SECS * 0.95;
                    let usable_w = plot_w * 0.95;
                    let now_t = start.elapsed().as_secs_f64();
                    let start_t = if now_t > fill_span { now_t - fill_span } else { 0.0 };

                    // Axis label (Y only)
                    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
                    cr.set_font_size(12.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.55);

                    // Y-axis label (rotated, centered on left)
                    cr.save().ok();
                    cr.translate(10.0, h / 2.0);
                    cr.rotate(-std::f64::consts::FRAC_PI_2);
                    let y_label = "Core Clock (MHz)";
                    let (yext_w, yext_xb) = match cr.text_extents(y_label) {
                        Ok(ext) => (ext.width(), ext.x_bearing()),
                        Err(_) => (0.0, 0.0),
                    };
                    cr.move_to(-yext_w / 2.0 - yext_xb, -label_pad);
                    let _ = cr.show_text(y_label);
                    cr.restore().ok();

                    let st = history.borrow();
                    if st.points.is_empty() {
                        return;
                    }

                    // Y axis is always anchored at 0 MHz so the axes intersection is (0,0).
                    // (i.e., the chart never shifts below 0 even if we add headroom.)
                    let min_v: f64 = 0.0;
                    let mut max_v: f64 = 0.0;
                    let end_t = start_t + fill_span;
                    for &(_t, v) in st.points.iter() {
                        if v.is_finite() {
                            max_v = max_v.max(v);
                        }
                    }

                    if max_v <= 0.0 {
                        max_v = 1.0;
                    } else {
                        max_v += max_v * 0.08;
                    }

                    let denom = (max_v - min_v).max(1e-9_f64);

                    // Subtle grid + Y tick labels for each grid line
                    cr.set_line_width(1.0);
                    for i in 0..=4 {
                        let t = (i as f64) / 4.0;
                        let y = pad + plot_h * t;

                        // Grid line
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
                        cr.move_to(pad, y);
                        cr.line_to(pad + plot_w, y);
                        let _ = cr.stroke();

                        // Tick label (value at this grid line)
                        let value = max_v - (max_v - min_v) * t;
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.45);
                        cr.set_font_size(11.0);
                        cr.move_to(pad + 4.0, y - 2.0);
                        let _ = cr.show_text(&format!("{:.0}", value));
                    }

                    // Axes (origin at bottom-left of plot area is always (0,0))
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
                    cr.set_line_width(1.0);
                    // Y axis
                    cr.move_to(pad, pad);
                    cr.line_to(pad, pad + plot_h);
                    // X axis (at 0 MHz)
                    let y0 = pad + plot_h;
                    cr.move_to(pad, y0);
                    cr.line_to(pad + usable_w, y0);
                    let _ = cr.stroke();

                    // Line
                    cr.set_line_width(2.0);
                    cr.set_line_cap(cairo::LineCap::Round);
                    cr.set_line_join(cairo::LineJoin::Round);
                    // Orange hue that remains readable in light and dark.
                    cr.set_source_rgba(0.95, 0.55, 0.20, 1.0);

                    // Build a visible polyline.
                    let last_y = st.points.back().map(|p| p.1).unwrap_or(0.0);

                    let mut started = false;
                    for &(t, v) in st.points.iter() {
                        if t < start_t {
                            continue;
                        }
                        if t > end_t {
                            break;
                        }
                        let x = pad + ((t - start_t) / fill_span).clamp(0.0, 1.0) * usable_w;
                        let y = pad + (1.0 - ((v - min_v) / denom).clamp(0.0, 1.0)) * plot_h;
                        if !started {
                            cr.move_to(x, y);
                            started = true;
                        } else {
                            cr.line_to(x, y);
                        }
                    }

                    // Extend to "now" so the line visually grows between 1Hz samples.
                    let t_now = now_t.min(end_t);
                    if t_now >= start_t {
                        let x = pad + ((t_now - start_t) / fill_span).clamp(0.0, 1.0) * usable_w;
                        let y = pad + (1.0 - ((last_y - min_v) / denom).clamp(0.0, 1.0)) * plot_h;
                        if !started {
                            cr.move_to(x, y);
                        } else {
                            cr.line_to(x, y);
                        }
                    }
                    let _ = cr.stroke();
                });
            }

            // Redraw both charts at ~30fps so they feel live even though samples are 1Hz.
            {
                let core = core_chart.clone();
                let mem = mem_chart.clone();
                glib::timeout_add_local(Duration::from_millis(33), move || {
                    core.queue_draw();
                    mem.queue_draw();
                    glib::Continue(true)
                });
            }

            let charts_row = Grid::new();
            charts_row.set_column_spacing(12);
            charts_row.set_row_spacing(12);
            charts_row.set_hexpand(true);
            charts_row.set_column_homogeneous(true);
            charts_row.attach(&core_chart_card, 0, 0, 1, 1);
            charts_row.attach(&mem_chart_card, 1, 0, 1, 1);

            metrics_layout.append(&top_grid);
            metrics_layout.append(&charts_row);

            metrics_box.append(&metrics_layout);

            // Libadwaita-style tabs (ViewStack + ViewSwitcherTitle)
            let stack = adw::ViewStack::new();
            let perf_page = stack.add_titled(&perf_box, Some("performance"), "Performance");
            perf_page.set_icon_name(Some("preferences-system-symbolic"));

            let metrics_page = stack.add_titled(&metrics_box, Some("metrics"), "Metrics");
            metrics_page.set_icon_name(Some("utilities-system-monitor-symbolic"));

            let switcher = adw::ViewSwitcherTitle::new();
            switcher.set_stack(Some(&stack));

            let header = adw::HeaderBar::new();
            header.set_title_widget(Some(&switcher));

            // Explicit window controls on the right.
            header.set_show_end_title_buttons(false);
            header.set_show_start_title_buttons(false);
            let controls = WindowControls::new(PackType::End);
            controls.set_decoration_layout(Some("minimize,maximize,close"));
            header.pack_end(&controls);

            stack.set_hexpand(true);
            stack.set_vexpand(true);

            let root = GtkBox::new(Orientation::Vertical, 0);
            root.set_hexpand(true);
            root.set_vexpand(true);
            root.append(&header);
            root.append(&stack);

            // Add a small CSS provider to increase progress bar height
            if let Some(display) = gdk::Display::default() {
                let provider = CssProvider::new();
                let css = ".metrics-card { padding: 12px; }\n.metrics-card-snug { padding: 8px; }\n.metrics-title { font-weight: 700; }\n.metrics-stat-name { opacity: 0.9; }\n.metrics-stat-value { font-weight: 700; }\n.metrics-row { padding: 6px 10px; }\n.metrics-gauge-big { font-size: 32px; font-weight: 700; }\n.metrics-gauge-mid { font-size: 22px; font-weight: 700; }\n.metrics-gauge-small { opacity: 0.85; }\n.metrics-value { font-weight: 700; }\nlevelbar.nvidia-progress trough { min-height: 28px; border-radius: 8px; }\nlevelbar.nvidia-progress trough block { min-height: 28px; border-radius: 8px; }\nlevelbar.nvidia-progress trough block.filled { background-color: @accent_bg_color; }\nlevelbar.nvidia-progress trough block.empty { background-color: alpha(@window_fg_color, 0.06); }\n\n.perf-section-title { font-weight: 700; font-size: 18px; }\n.perf-card { padding: 14px; }\n.perf-icon-badge { min-width: 36px; min-height: 36px; border-radius: 999px; background-color: transparent; }\n.perf-card-title { font-weight: 700; }\n.perf-card-subtitle { opacity: 0.75; }\n.perf-row { padding: 6px 6px; }\n.perf-row-title { font-weight: 600; }\n.perf-row-value { font-weight: 700; min-width: 56px; }\n/* Make numeric entry fields visually match surrounding cards (no native entry chrome) */\nentry.perf-row-value { background-color: transparent; border: none; padding: 0; }\nentry.perf-row-value:focus { background-color: transparent; box-shadow: none; }\n.perf-stepper { border-radius: 999px; padding: 6px 12px; background-color: alpha(@window_fg_color, 0.06); }\n.perf-disclosure { border-radius: 999px; padding: 4px 10px; background-color: alpha(@window_fg_color, 0.06); }\n.perf-action-primary { border-radius: 999px; padding: 10px 18px; }\n.perf-action-secondary { border-radius: 999px; padding: 10px 18px; }";
                provider.load_from_data(css);
                gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
            }

            // Live preview updater
            let preview_buffer = preview.buffer();
            let gpu_combo_clone = gpu_combo.clone();
            // Clone adjustments so closures can capture them without moving original values
            let pa = power_adj.clone();
            let fa = freq_adj.clone();
            let ma = mem_adj.clone();
            let mia = min_adj.clone();
            let mxa = max_adj.clone();
            let preview_updater = move || {
                let active = gpu_combo_clone.active_id();
                let gpu_id = active.as_deref().unwrap_or("0");
                let cmd = build_command(gpu_id, (pa.value() * 1000.0).round() as i32, fa.value() as i32, ma.value() as i32, mia.value() as i32, mxa.value() as i32);
                preview_buffer.set_text(&cmd);
            };

            let preview_updater_clone = preview_updater.clone();
            power_adj.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            freq_adj.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            mem_adj.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            min_adj.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            max_adj.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            gpu_combo.connect_changed(move |_| preview_updater_clone());

            preview_updater();

            // Disable the service button initially if a service exists and
            // the current NVML-derived values match the values stored in
            // the service. Re-enable the button as soon as the user
            // changes any control.
            let service_exists = std::path::Path::new(svc_path).exists();
            let svc_index_clone = svc_index.clone();
            let svc_power_clone = svc_power;
            let svc_freq_clone = svc_freq;
            let svc_mem_clone = svc_mem;
            let svc_min_clone = svc_min;
            let svc_max_clone = svc_max;
            let service_btn_state = service_btn.clone();
            let combo_state = gpu_combo.clone();
            let pa_state = power_adj.clone();
            let fa_state = freq_adj.clone();
            let ma_state = mem_adj.clone();
            let mia_state = min_adj.clone();
            let mxa_state = max_adj.clone();

            let check_state = Rc::new(move || {
                if !service_exists {
                    service_btn_state.set_sensitive(true);
                    return;
                }
                let active = combo_state.active_id();
                let gpu_id = active.as_deref().unwrap_or("0");
                let power_now = (pa_state.value() * 1000.0).round() as i32;
                let freq_now = fa_state.value() as i32;
                let mem_now = ma_state.value() as i32;
                let min_now = mia_state.value() as i32;
                let max_now = mxa_state.value() as i32;

                let idx_eq = svc_index_clone.as_deref().map(|s| s == gpu_id).unwrap_or(false);
                let power_eq = svc_power_clone.map(|p| p == power_now).unwrap_or(false);
                let freq_eq = svc_freq_clone.map(|p| p == freq_now).unwrap_or(false);
                let mem_eq = svc_mem_clone.map(|p| p == mem_now).unwrap_or(false);
                let min_eq = svc_min_clone.map(|p| p == min_now).unwrap_or(false);
                let max_eq = svc_max_clone.map(|p| p == max_now).unwrap_or(false);

                if idx_eq && power_eq && freq_eq && mem_eq && min_eq && max_eq {
                    service_btn_state.set_sensitive(false);
                } else {
                    service_btn_state.set_sensitive(true);
                }
            });

            // Attach the checker to all controls so any change will re-evaluate
            // the service button state.
            let cs = check_state.clone();
            power_adj.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            freq_adj.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            mem_adj.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            min_adj.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            max_adj.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            gpu_combo.connect_changed(move |_| cs());

            // Run once to set initial state
            (check_state)();

            // NVML handle for periodic metric updates (if available)
            let nvml_handle = nvml_wrapper::Nvml::init().ok();

            // Poll NVML every second to update metrics widgets
            let gauge_state_cl = gauge_state.clone();
            let vram_center_used_cl = vram_center_used.clone();
            let vram_center_total_cl = vram_center_total.clone();
            let vram_gauge_cl = vram_gauge.clone();

            let stat_core_value_cl = stat_core_value.clone();
            let stat_mem_value_cl = stat_mem_value.clone();
            let stat_temp_value_cl = stat_temp_value.clone();

            let core_clock_history_cl = core_clock_history.clone();
            let mem_clock_history_cl = mem_clock_history.clone();
            let chart_start_cl = chart_start;

            let usage_bar_cl = usage_bar.clone();
            let usage_value_cl = usage_value.clone();

            let power_bar_cl = power_bar.clone();
            let power_value_cl = power_value.clone();

            let fan_value_cl = fan_center_pct.clone();
            let fan_rpm_value_cl = fan_center_rpm.clone();
            let fan_gauge_cl = fan_gauge.clone();

            // Move nvml_handle into the timeout closure so it remains alive
            let nvml_handle = nvml_handle;
            glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                if let Some(ref nvml) = nvml_handle {
                    if let Ok(dev) = nvml.device_by_index(gpu_index_num) {
                        // VRAM
                        if let Ok(mi) = dev.memory_info() {
                            let used_mib = mi.used / 1024 / 1024;
                            let total_mib = mi.total / 1024 / 1024;
                            {
                                let mut st = gauge_state_cl.borrow_mut();
                                st.vram_used_mib = used_mib as f64;
                                st.vram_total_mib = total_mib as f64;
                            }
                            vram_center_used_cl.set_text(&format!("{}", used_mib));
                            vram_center_total_cl.set_text(&format!("/ {} MiB", total_mib));
                            vram_gauge_cl.queue_draw();
                        } else {
                            {
                                let mut st = gauge_state_cl.borrow_mut();
                                st.vram_used_mib = 0.0;
                                st.vram_total_mib = 0.0;
                            }
                            vram_center_used_cl.set_text("N/A");
                            vram_center_total_cl.set_text("");
                            vram_gauge_cl.queue_draw();
                        }

                        // Clocks
                        match dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics) {
                            Ok(clk) => {
                                stat_core_value_cl.set_text(&format!("{} MHz", clk));

                                // Push into history (time-based)
                                {
                                    let mut st = core_clock_history_cl.borrow_mut();
                                    let t = chart_start_cl.elapsed().as_secs_f64();
                                    st.points.push_back((t, clk as f64));
                                    while st.points.len() > 300 {
                                        st.points.pop_front();
                                    }
                                }
                            }
                            Err(_) => {
                                stat_core_value_cl.set_text("N/A");
                            }
                        }

                        match dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory) {
                            Ok(clk) => {
                                stat_mem_value_cl.set_text(&format!("{} MHz", clk));
                                {
                                    let mut st = mem_clock_history_cl.borrow_mut();
                                    let t = chart_start_cl.elapsed().as_secs_f64();
                                    st.points.push_back((t, clk as f64));
                                    while st.points.len() > 300 {
                                        st.points.pop_front();
                                    }
                                }
                            }
                            Err(_) => {
                                stat_mem_value_cl.set_text("N/A");
                            }
                        }

                        // Temperature (GPU)
                        let temp_text = match dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu) {
                            Ok(g) => format!("{} °C", g),
                            Err(_) => "N/A".to_string(),
                        };
                        stat_temp_value_cl.set_text(&temp_text);

                        // Fan Speed (try multiple fan indices; use first successful reading)
                        let mut fans_text = "N/A".to_string();
                        let mut fan_rpm_text = "N/A".to_string();
                        let mut fan_pct_opt: Option<f64> = None;
                        for i in 0u32..4u32 {
                            if let Ok(s) = dev.fan_speed(i) {
                                fan_pct_opt = Some(s as f64);
                                fans_text = format!("{}%", s);
                                break;
                            }
                        }

                        // Fan RPM (may be unsupported on some devices/drivers)
                        for i in 0u32..4u32 {
                            if let Ok(rpm) = dev.fan_speed_rpm(i) {
                                fan_rpm_text = format!("{} RPM", rpm);
                                break;
                            }
                        }
                        fan_value_cl.set_text(&fans_text);
                        fan_rpm_value_cl.set_text(&fan_rpm_text);
                        {
                            let mut st = gauge_state_cl.borrow_mut();
                            st.fan_pct = fan_pct_opt.unwrap_or(0.0);
                        }
                        fan_gauge_cl.queue_draw();

                        // Utilization -> usage progress bar
                        if let Ok(u) = dev.utilization_rates() {
                            usage_bar_cl.set_min_value(0.0);
                            usage_bar_cl.set_max_value(100.0);
                            usage_bar_cl.set_value(u.gpu as f64);
                            usage_value_cl.set_text(&format!("{}%", u.gpu));
                        } else {
                            usage_bar_cl.set_value(0.0);
                            usage_value_cl.set_text("N/A");
                        }

                        // Power usage -> power progress bar
                        match (dev.power_usage(), dev.enforced_power_limit()) {
                            (Ok(cur), Ok(limit)) if limit > 0 => {
                                let cur_w = (cur as f64) / 1000.0;
                                let lim_w = (limit as f64) / 1000.0;
                                power_bar_cl.set_min_value(0.0);
                                power_bar_cl.set_max_value(lim_w);
                                power_bar_cl.set_value(cur_w);
                                power_value_cl.set_text(&format!("{:.2} / {:.2} W", cur_w, lim_w));
                            }
                            (Ok(cur), Ok(limit)) => {
                                let cur_w = (cur as f64) / 1000.0;
                                let lim_w = (limit as f64) / 1000.0;
                                power_bar_cl.set_min_value(0.0);
                                power_bar_cl.set_max_value(lim_w);
                                power_bar_cl.set_value(0.0);
                                power_value_cl.set_text(&format!("{:.2} / {:.2} W", cur_w, lim_w));
                            }
                            (Ok(cur), Err(_)) => {
                                let cur_w = (cur as f64) / 1000.0;
                                power_bar_cl.set_min_value(0.0);
                                power_bar_cl.set_max_value(cur_w.max(1.0));
                                power_bar_cl.set_value(cur_w);
                                power_value_cl.set_text(&format!("{:.2} W", cur_w));
                            }
                            _ => {
                                power_bar_cl.set_min_value(0.0);
                                power_bar_cl.set_max_value(1.0);
                                power_bar_cl.set_value(0.0);
                                power_value_cl.set_text("N/A");
                            }
                        }
                    }
                }
                glib::Continue(true)
            });

            // Service button handler
            let service_btn_clone = service_btn.clone();
            let gpu_for_service = gpu_combo.clone();
            let window_for_service = window.clone();
            // clones for service handler
            let pa2 = power_adj.clone();
            let fa2 = freq_adj.clone();
            let ma2 = mem_adj.clone();
            let mia2 = min_adj.clone();
            let mxa2 = max_adj.clone();
            service_btn.connect_clicked(move |_| {
                let active = gpu_for_service.active_id();
                let gpu_id = active.as_deref().unwrap_or("0");
                let cmd = build_command(gpu_id, (pa2.value() * 1000.0).round() as i32, fa2.value() as i32, ma2.value() as i32, mia2.value() as i32, mxa2.value() as i32);

                let service_path = "/etc/systemd/system/zelos.service";
                let exists = std::path::Path::new(service_path).exists();

                let content = format!("[Unit]\nDescription=NVIDIA Overclocking Service\nAfter=network.target\n\n[Service]\nExecStart={}\nUser=root\nRestart=on-failure\n\n[Install]\nWantedBy=multi-user.target\n", cmd);

                let tmp = std::env::temp_dir().join("zelos.service.tmp");
                if let Err(e) = std::fs::write(&tmp, content) {
                    show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to write temp service file: {}", e));
                    return;
                }

                // Create a small helper script that performs the install + systemctl steps.
                // This script will be executed once via `pkexec`, so the user is prompted for
                // elevation only a single time. If elevation is denied, nothing is performed.
                let script_path = std::env::temp_dir().join("zelos_install.sh");
                let action = if !exists { "create" } else { "update" };
                                let script = r#"#!/bin/sh
                set -e
                SERVICE_PATH=/etc/systemd/system/zelos.service
                TMP_SERVICE="$1"
                ACTION="$2"
                if ! install -m 644 "$TMP_SERVICE" "$SERVICE_PATH"; then
                    echo "failed to install service" >&2
                    exit 2
                fi
                if ! systemctl daemon-reload; then
                    echo "failed to daemon-reload" >&2
                    exit 3
                fi
                if [ "$ACTION" = "create" ]; then
                    if ! systemctl enable --now zelos; then
                        echo "failed to enable/start service" >&2
                        exit 4
                    fi
                else
                    if ! systemctl restart zelos; then
                        echo "failed to restart service" >&2
                        exit 5
                    fi
                fi
                exit 0
                "#.to_string();

                if let Err(e) = std::fs::write(&script_path, script) {
                    show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to write helper script: {}", e));
                    return;
                }
                if let Err(e) = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)) {
                    show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to set exec perms on helper script: {}", e));
                    return;
                }

                let result = std::process::Command::new("pkexec").arg(script_path.to_str().unwrap()).arg(tmp.to_str().unwrap()).arg(action).output();
                match result {
                    Ok(out) => {
                            if out.status.success() {
                            show_message(Some(&window_for_service), MessageType::Info, ButtonsType::Ok, if !exists { "Service created, enabled and started." } else { "Service updated and restarted." });
                            // Update the service button text. Calling `set_label`
                            // is a reliable way to change the visible label even
                            // when the button contains a custom child on many
                            // themes.
                            service_btn_clone.set_label("Update Service");
                        } else {
                            let mut msg = String::new();
                            if !out.stdout.is_empty() {
                                msg.push_str(&String::from_utf8_lossy(&out.stdout));
                            }
                            if !out.stderr.is_empty() {
                                if !msg.is_empty() { msg.push('\n'); }
                                msg.push_str(&String::from_utf8_lossy(&out.stderr));
                            }
                            if msg.is_empty() {
                                msg = format!("Process exited with status: {}", out.status);
                            }
                            show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to install/update service: {}", msg));
                        }
                    }
                    Err(e) => {
                        show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to run pkexec: {}", e));
                    }
                }

                // Cleanup temporary files regardless of success or failure. Ignore errors.
                let _ = std::fs::remove_file(&tmp);
                let _ = std::fs::remove_file(&script_path);
            });

            // Apply handler
            let window_clone = window.clone();
            let preview_clone = preview.clone();
            apply.connect_clicked(move |_| {
                let buffer = preview_clone.buffer();
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, true).to_string();

                if text.trim().is_empty() {
                    show_message(Some(&window_clone), MessageType::Warning, ButtonsType::Ok, "Command is empty");
                    return;
                }

                let confirm = MessageDialog::new(Some(&window_clone), gtk4::DialogFlags::MODAL, MessageType::Question, ButtonsType::YesNo, "Are you sure?");
                confirm.set_secondary_text(Some("This will apply any changes you've made to your GPU"));
                // center any label children inside the dialog's content area
                let area = confirm.content_area();
                // remove any auto-created children so we can insert our own centered labels
                while let Some(ch) = area.first_child() {
                    area.remove(&ch);
                }
                // Insert custom centered labels. Use CSS to make the headline bold and 1px larger.
                if let Some(display) = gdk::Display::default() {
                    let provider = CssProvider::new();
                    // Keep context at the app's default font size; make headline bold and 2px larger
                    let css = "#confirm-title { font-weight: bold; font-size: calc(100% + 2px); } #confirm-context { padding-top:6px; }";
                    provider.load_from_data(css);
                    gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
                }

                // Headline label
                let title_label = Label::new(None);
                title_label.set_markup("<b>Are you sure?</b>");
                title_label.set_halign(gtk4::Align::Center);
                title_label.set_valign(gtk4::Align::Center);
                title_label.set_widget_name("confirm-title");
                title_label.set_margin_bottom(6);
                // Context label
                let context_label = Label::new(Some("This will apply any changes you've made to your GPU"));
                context_label.set_halign(gtk4::Align::Center);
                context_label.set_valign(gtk4::Align::Center);
                context_label.set_widget_name("confirm-context");
                context_label.set_wrap(true);
                area.append(&title_label);
                area.append(&context_label);
                // Slightly increase dialog size so there's margin between window edge and text
                confirm.set_default_size(520, 140);

                let win_resp = window_clone.clone();
                let preview_resp = preview_clone.clone();
                confirm.connect_response(move |dlg, resp| {
                    dlg.close();
                    if resp == gtk4::ResponseType::Yes {
                        let buffer = preview_resp.buffer();
                        let start = buffer.start_iter();
                        let end = buffer.end_iter();
                        let text = buffer.text(&start, &end, true).to_string();
                        let mut parts: Vec<&str> = text.split_whitespace().collect();
                        if parts.is_empty() { return; }
                        let program = parts.remove(0);
                        let args: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                        let result = std::process::Command::new("pkexec").arg(program).args(&args).output();

                        match result {
                            Ok(out) => {
                                let mut shown = String::new();
                                if !out.stdout.is_empty() {
                                    shown.push_str("STDOUT:\n");
                                    shown.push_str(&String::from_utf8_lossy(&out.stdout));
                                }
                                if !out.stderr.is_empty() {
                                    shown.push_str("\nSTDERR:\n");
                                    shown.push_str(&String::from_utf8_lossy(&out.stderr));
                                }
                                if shown.is_empty() { shown = format!("Process exited with status: {}", out.status); }
                                show_message(Some(&win_resp), MessageType::Info, ButtonsType::Ok, &shown);
                            }
                            Err(e) => {
                                show_message(Some(&win_resp), MessageType::Error, ButtonsType::Ok, &format!("Failed to execute: {}", e));
                            }
                        }
                    }
                });
                confirm.present();
            });

            window.set_content(Some(&root));

            // Size the window based on the Metrics tab's natural size, then
            // force the width down by 30% and lock it so live updates never widen it.
            let prev_page = stack.visible_child_name().map(|s| s.to_string());
            stack.set_visible_child_name("metrics");
            // First measure the natural size, then re-measure minimum width using a
            // concrete height. This avoids asking GTK to measure with a width that
            // is below the true minimum for the resulting height.
            let (_min_w0, nat_w0, _min_base0, _nat_base0) = root.measure(Orientation::Horizontal, -1);
            let (_min_h0, nat_h0, _min_base_h0, _nat_base_h0) = root.measure(Orientation::Vertical, nat_w0);
            let (min_w, nat_w, _min_base, _nat_base) = root.measure(Orientation::Horizontal, nat_h0);

            let target_w = (((nat_w as f64) * 0.70).round() as i32).max(min_w);
            let (_min_h, nat_h, _min_base_h, _nat_base_h) = root.measure(Orientation::Vertical, target_w);
            if target_w > 0 && nat_h > 0 {
                window.set_default_size(target_w, nat_h);
                // Prevent dynamic content from increasing the width.
                window.set_size_request(target_w, -1);
            }
            if let Some(prev) = prev_page {
                stack.set_visible_child_name(&prev);
            } else {
                stack.set_visible_child_name("performance");
            }
            window.show();
        });

        app.run();
    }
}

#[cfg(feature = "gui")]
pub use imp::run;

#[cfg(not(feature = "gui"))]
pub fn run(_config_path: &str) {
    eprintln!("GUI feature not enabled. Rebuild with `--features gui` to enable the GTK4 GUI.");
}
