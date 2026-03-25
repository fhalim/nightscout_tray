use std::io;
use std::sync::mpsc;
use std::thread;

use eframe::egui;

use crate::config::AppConfig;

pub fn open_settings_dialog(current: &AppConfig) -> io::Result<Option<AppConfig>> {
    let (sender, receiver) = mpsc::channel();
    let current = current.clone();

    thread::Builder::new()
        .name("nightscout-settings".to_string())
        .spawn(move || {
            let fallback_sender = sender.clone();
            let result = eframe::run_native(
                "NightScout Settings",
                dialog_options([420.0, 240.0]),
                Box::new(move |_cc| Ok(Box::new(SettingsApp::new(current, sender)))),
            );

            if let Err(error) = result {
                let _ = fallback_sender.send(Err(io::Error::other(error.to_string())));
            }
        })?;

    receiver.recv().map_err(|error| {
        io::Error::other(format!("settings dialog closed unexpectedly: {error}"))
    })?
}

pub fn show_error_dialog(message: &str) {
    let message = message.to_string();

    let _ = thread::Builder::new()
        .name("nightscout-error".to_string())
        .spawn(move || {
            let _ = eframe::run_native(
                "NightScout Error",
                dialog_options([360.0, 180.0]),
                Box::new(move |_cc| Ok(Box::new(ErrorApp::new(message)))),
            );
        });
}

fn dialog_options(size: [f32; 2]) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_resizable(false),
        ..Default::default()
    }
}

struct SettingsApp {
    nightscout_url: String,
    api_token: String,
    refresh_minutes: u64,
    launch_on_startup: bool,
    thresholds: crate::config::GlucoseThresholds,
    result_sender: Option<mpsc::Sender<io::Result<Option<AppConfig>>>>,
}

impl SettingsApp {
    fn new(current: AppConfig, result_sender: mpsc::Sender<io::Result<Option<AppConfig>>>) -> Self {
        Self {
            nightscout_url: current.nightscout_url,
            api_token: current.api_token,
            refresh_minutes: current.refresh_minutes,
            launch_on_startup: current.launch_on_startup,
            thresholds: current.thresholds,
            result_sender: Some(result_sender),
        }
    }

    fn finish(&mut self, result: io::Result<Option<AppConfig>>, ctx: &egui::Context) {
        if let Some(sender) = self.result_sender.take() {
            let _ = sender.send(result);
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl Drop for SettingsApp {
    fn drop(&mut self) {
        if let Some(sender) = self.result_sender.take() {
            let _ = sender.send(Ok(None));
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("NightScout Settings");
            ui.add_space(8.0);

            ui.label("NightScout URL");
            ui.text_edit_singleline(&mut self.nightscout_url);
            ui.add_space(8.0);

            ui.label("API token");
            ui.add(egui::TextEdit::singleline(&mut self.api_token).password(true));
            ui.add_space(8.0);

            ui.label("Refresh frequency in minutes");
            ui.add(egui::Slider::new(&mut self.refresh_minutes, 1..=120).integer());

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    self.finish(Ok(None), ctx);
                }

                if ui.button("Save").clicked() {
                    let config = AppConfig {
                        nightscout_url: self.nightscout_url.clone(),
                        api_token: self.api_token.clone(),
                        refresh_minutes: self.refresh_minutes,
                        launch_on_startup: self.launch_on_startup,
                        thresholds: self.thresholds.clone(),
                    }
                    .normalized();

                    self.finish(Ok(Some(config)), ctx);
                }
            });
        });
    }
}

struct ErrorApp {
    message: String,
}

impl ErrorApp {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl eframe::App for ErrorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("NightScout Error");
            ui.add_space(8.0);
            ui.add(egui::Label::new(self.message.as_str()).wrap());
            ui.add_space(16.0);

            if ui.button("Close").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}
