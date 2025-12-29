// Clean, redesigned GTK4 GUI
#![allow(dead_code)]

#[cfg(feature = "gui")]
pub mod imp {
    use gtk4::prelude::*;
    use gtk4::{Application, ApplicationWindow, Box as GtkBox, Orientation, Label, Button, Scale, Adjustment, SpinButton, TextView, ScrolledWindow, MessageDialog, MessageType, ButtonsType, Grid, ComboBoxText, CssProvider};
    use gtk4::gdk;

    fn build_command(gpu_index: &str, power: i32, freq: i32, mem: i32, min_clock: i32, max_clock: i32) -> String {
        let prog = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "nvidia_oc".to_string());
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
        let svc_path = "/etc/systemd/system/nvidia_oc.service";
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

        let app = Application::new(Some("org.github.kombatant.nvidia_oc"), Default::default());

        app.connect_activate(move |app| {
            let window = ApplicationWindow::new(app);
            window.set_title(Some("Nvidia OC"));

            // Note: removed elevated-action CSS provider — no red border shown.
            // Let GTK size the window to the content automatically

            // Slightly tighter vertical spacing to reduce empty space
            let main = GtkBox::new(Orientation::Vertical, 8);
            main.set_margin_top(6);
            main.set_margin_bottom(6);
            main.set_margin_start(6);
            main.set_margin_end(6);

            // GPU selector
            let gpu_box = GtkBox::new(Orientation::Horizontal, 6);
            // make the GPU selector expand to match the width of the grid below
            gpu_box.set_hexpand(true);
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
            gpu_box.append(&gpu_combo);
            main.append(&gpu_box);

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

            // Controls grid
            let grid = Grid::new();
            grid.set_row_spacing(10);
            grid.set_column_spacing(20);
            grid.set_hexpand(true);

            let power_label = Label::new(Some("Power (W)"));
            power_label.set_halign(gtk4::Align::Start);
            let power_initial = match current_power {
                Some(p) => p as f64 / 1000.0,
                None => svc_power.unwrap_or(400_000) as f64 / 1000.0,
            };
            let power_adj = Adjustment::new(power_initial, 0.0, 500.0, 0.1, 1.0, 0.0);
            let power_scale = Scale::new(Orientation::Horizontal, Some(&power_adj));
            power_scale.set_hexpand(true);
            let power_spin = SpinButton::new(Some(&power_adj), 0.1, 1);
            grid.attach(&power_label, 0, 0, 1, 1);
            grid.attach(&power_scale, 1, 0, 1, 1);
            grid.attach(&power_spin, 2, 0, 1, 1);

            let freq_label = Label::new(Some("GPU freq offset (MHz)"));
            freq_label.set_halign(gtk4::Align::Start);
            let freq_initial = current_freq.unwrap_or(svc_freq.unwrap_or(0));
            let freq_adj = Adjustment::new(freq_initial as f64, -2000.0, 2000.0, 1.0, 10.0, 0.0);
            let freq_spin = SpinButton::new(Some(&freq_adj), 1.0, 0);
            let mem_label = Label::new(Some("Memory offset (MHz)"));
            mem_label.set_halign(gtk4::Align::Start);
            let mem_initial = current_mem.unwrap_or(svc_mem.unwrap_or(0));
            let mem_adj = Adjustment::new(mem_initial as f64, -20000.0, 20000.0, 1.0, 10.0, 0.0);
            let mem_spin = SpinButton::new(Some(&mem_adj), 1.0, 0);
            grid.attach(&freq_label, 0, 1, 1, 1);
            // span spin controls across columns 1 and 2 so each row has the
            // same overall width as the power row (which uses two control columns)
            grid.attach(&freq_spin, 1, 1, 2, 1);
            grid.attach(&mem_label, 0, 2, 1, 1);
            grid.attach(&mem_spin, 1, 2, 2, 1);

            let min_label = Label::new(Some("Min clock (MHz)"));
            min_label.set_halign(gtk4::Align::Start);
            let min_initial = svc_min.unwrap_or(0);
            let min_adj = Adjustment::new(min_initial as f64, 0.0, 5000.0, 1.0, 10.0, 0.0);
            let min_spin = SpinButton::new(Some(&min_adj), 1.0, 0);
            let max_label = Label::new(Some("Max clock (MHz)"));
            max_label.set_halign(gtk4::Align::Start);
            let max_initial = svc_max.unwrap_or(3800);
            let max_adj = Adjustment::new(max_initial as f64, 0.0, 5000.0, 1.0, 10.0, 0.0);
            let max_spin = SpinButton::new(Some(&max_adj), 1.0, 0);
            grid.attach(&min_label, 0, 3, 1, 1);
            grid.attach(&min_spin, 1, 3, 2, 1);
            grid.attach(&max_label, 0, 4, 1, 1);
            grid.attach(&max_spin, 1, 4, 2, 1);

            main.append(&grid);

            // Preview
            let preview_label = Label::new(Some("Command Preview"));
            preview_label.set_halign(gtk4::Align::Start);
            main.append(&preview_label);
            let scrolled = ScrolledWindow::new();
            // Make preview tall enough for ~3 lines of text
            scrolled.set_min_content_height(84);
            scrolled.set_min_content_width(520);
            scrolled.set_vexpand(false);
            let preview = TextView::new();
            preview.set_editable(false);
            preview.set_monospace(true);
            preview.set_pixels_above_lines(2);
            preview.set_pixels_below_lines(2);
            preview.set_wrap_mode(gtk4::WrapMode::WordChar);
            scrolled.set_child(Some(&preview));
            main.append(&scrolled);

            // Actions
            let actions = GtkBox::new(Orientation::Horizontal, 8);
            actions.set_halign(gtk4::Align::End);
            // keep the total distance from the bottom edge equal to the top
            // combo box distance (main margin_top == 6). main has
            // `set_margin_bottom(6)` so make this child margin 0.
            actions.set_margin_bottom(0);
            // Create service button with explicit lock icon label so the icon
            // is always visible (GTK CSS ::before may not be supported by theme)
            let service_btn = Button::new();
            let svc_text = if std::path::Path::new(svc_path).exists() { "Update Service" } else { "Create Service" };
            let service_icon_lbl = Label::new(Some("🔒"));
            service_icon_lbl.set_margin_end(6);
            let service_text_lbl = Label::new(Some(svc_text));
            let service_box = GtkBox::new(Orientation::Horizontal, 6);
            service_box.append(&service_icon_lbl);
            service_box.append(&service_text_lbl);
            service_btn.set_child(Some(&service_box));
            // no elevated-action class; keep default classes only

            // Apply button with icon as well
            let apply = Button::new();
            let apply_icon_lbl = Label::new(Some("🔒"));
            apply_icon_lbl.set_margin_end(6);
            let apply_text_lbl = Label::new(Some("Apply"));
            let apply_box = GtkBox::new(Orientation::Horizontal, 6);
            apply_box.append(&apply_icon_lbl);
            apply_box.append(&apply_text_lbl);
            apply.set_child(Some(&apply_box));
            apply.set_css_classes(&["suggested-action"]);

            actions.append(&service_btn);
            actions.append(&apply);
            main.append(&actions);

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
            power_scale.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            freq_spin.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            mem_spin.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            min_spin.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            max_spin.connect_value_changed(move |_| preview_updater_clone());
            let preview_updater_clone = preview_updater.clone();
            gpu_combo.connect_changed(move |_| preview_updater_clone());

            preview_updater();

            // Disable the service button initially if a service exists and
            // the current NVML-derived values match the values stored in
            // the service. Re-enable the button as soon as the user
            // changes any control.
            use std::rc::Rc;
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
            power_scale.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            freq_spin.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            mem_spin.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            min_spin.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            max_spin.connect_value_changed(move |_| cs());
            let cs = check_state.clone();
            gpu_combo.connect_changed(move |_| cs());

            // Run once to set initial state
            (check_state)();

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

                let service_path = "/etc/systemd/system/nvidia_oc.service";
                let exists = std::path::Path::new(service_path).exists();

                let content = format!("[Unit]\nDescription=NVIDIA Overclocking Service\nAfter=network.target\n\n[Service]\nExecStart={}\nUser=root\nRestart=on-failure\n\n[Install]\nWantedBy=multi-user.target\n", cmd);

                let tmp = std::env::temp_dir().join("nvidia_oc.service.tmp");
                if let Err(e) = std::fs::write(&tmp, content) {
                    show_message(Some(&window_for_service), MessageType::Error, ButtonsType::Ok, &format!("Failed to write temp service file: {}", e));
                    return;
                }

                // Create a small helper script that performs the install + systemctl steps.
                // This script will be executed once via `pkexec`, so the user is prompted for
                // elevation only a single time. If elevation is denied, nothing is performed.
                let script_path = std::env::temp_dir().join("nvidia_oc_install.sh");
                let action = if !exists { "create" } else { "update" };
                                let script = r#"#!/bin/sh
set -e
SERVICE_PATH=/etc/systemd/system/nvidia_oc.service
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
    if ! systemctl enable --now nvidia_oc; then
        echo "failed to enable/start service" >&2
        exit 4
    fi
else
    if ! systemctl restart nvidia_oc; then
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

            window.set_child(Some(&main));
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
