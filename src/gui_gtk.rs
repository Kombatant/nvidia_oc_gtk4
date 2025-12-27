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

        let app = Application::new(Some("org.example.nvidia_oc"), Default::default());

        app.connect_activate(move |app| {
            let window = ApplicationWindow::new(app);
            window.set_title(Some("Nvidia OC"));
            window.set_default_size(820, 480);

            // Slightly tighter vertical spacing to reduce empty space
            let main = GtkBox::new(Orientation::Vertical, 8);
            main.set_margin_top(12);
            main.set_margin_bottom(8);
            main.set_margin_start(12);
            main.set_margin_end(12);

            // GPU selector
            let gpu_box = GtkBox::new(Orientation::Horizontal, 6);
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

            // Controls grid
            let grid = Grid::new();
            grid.set_row_spacing(10);
            grid.set_column_spacing(20);
            grid.set_hexpand(true);

            let power_label = Label::new(Some("Power (W)"));
            power_label.set_halign(gtk4::Align::Start);
            let power_initial = svc_power.unwrap_or(400_000) as f64 / 1000.0;
            let power_adj = Adjustment::new(power_initial, 0.0, 500.0, 0.1, 1.0, 0.0);
            let power_scale = Scale::new(Orientation::Horizontal, Some(&power_adj));
            power_scale.set_hexpand(true);
            let power_spin = SpinButton::new(Some(&power_adj), 0.1, 1);
            grid.attach(&power_label, 0, 0, 1, 1);
            grid.attach(&power_scale, 1, 0, 1, 1);
            grid.attach(&power_spin, 2, 0, 1, 1);

            let freq_label = Label::new(Some("GPU freq offset (MHz)"));
            freq_label.set_halign(gtk4::Align::Start);
            let freq_initial = svc_freq.unwrap_or(0);
            let freq_adj = Adjustment::new(freq_initial as f64, -2000.0, 2000.0, 1.0, 10.0, 0.0);
            let freq_spin = SpinButton::new(Some(&freq_adj), 1.0, 0);
            let mem_label = Label::new(Some("Memory offset (MHz)"));
            mem_label.set_halign(gtk4::Align::Start);
            let mem_initial = svc_mem.unwrap_or(0);
            let mem_adj = Adjustment::new(mem_initial as f64, -20000.0, 20000.0, 1.0, 10.0, 0.0);
            let mem_spin = SpinButton::new(Some(&mem_adj), 1.0, 0);
            grid.attach(&freq_label, 0, 1, 1, 1);
            grid.attach(&freq_spin, 1, 1, 1, 1);
            grid.attach(&mem_label, 0, 2, 1, 1);
            grid.attach(&mem_spin, 1, 2, 1, 1);

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
            grid.attach(&min_spin, 1, 3, 1, 1);
            grid.attach(&max_label, 0, 4, 1, 1);
            grid.attach(&max_spin, 1, 4, 1, 1);

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
            actions.set_margin_bottom(6);
            let service_btn = Button::with_label(if std::path::Path::new(svc_path).exists() { "Update Service" } else { "Create Service" });
            let apply = Button::with_label("Apply");
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

                let content = format!("[Unit]\nDescription=NVIDIA Overclocking Service\nAfter=network.target\n\n[Service]\nExecStart=\"{}\"\nUser=root\nRestart=on-failure\n\n[Install]\nWantedBy=multi-user.target\n", cmd);

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
