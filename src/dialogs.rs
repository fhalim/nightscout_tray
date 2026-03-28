use std::io;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eframe::egui;

use crate::config::{AppConfig, GlucoseThresholds};
use crate::nightscout::CgmEntry;
use crate::tray::color_for_reading;

static CHART_DIALOG: OnceLock<Mutex<Option<mpsc::Sender<ChartCommand>>>> = OnceLock::new();

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

pub fn toggle_chart_dialog(
    entries: Vec<CgmEntry>,
    thresholds: GlucoseThresholds,
) -> io::Result<()> {
    let state = CHART_DIALOG.get_or_init(|| Mutex::new(None));
    let mut active = state
        .lock()
        .map_err(|_| io::Error::other("chart dialog state is unavailable"))?;

    if let Some(sender) = active.as_ref() {
        if sender.send(ChartCommand::Close).is_ok() {
            return Ok(());
        }

        *active = None;
    }

    let (sender, receiver) = mpsc::channel();
    *active = Some(sender);

    if let Err(error) = thread::Builder::new()
        .name("nightscout-chart".to_string())
        .spawn(move || {
            let _ = eframe::run_native(
                "NightScout Chart",
                chart_options(),
                Box::new(move |_cc| Ok(Box::new(ChartApp::new(entries, thresholds, receiver)))),
            );

            if let Some(state) = CHART_DIALOG.get()
                && let Ok(mut active) = state.lock()
            {
                *active = None;
            }
        })
    {
        *active = None;
        return Err(error);
    }

    Ok(())
}

fn dialog_options(size: [f32; 2]) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_resizable(false),
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    {
        options.event_loop_builder = Some(Box::new(|builder| {
            winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(builder, true);
            winit::platform::x11::EventLoopBuilderExtX11::with_any_thread(builder, true);
        }));
    }

    options
}

fn chart_options() -> eframe::NativeOptions {
    let mut options = dialog_options([560.0, 400.0]);
    options.viewport = options.viewport.with_always_on_top();
    options
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

enum ChartCommand {
    Close,
}

struct ChartApp {
    entries: Vec<CgmEntry>,
    thresholds: GlucoseThresholds,
    receiver: mpsc::Receiver<ChartCommand>,
}

impl ChartApp {
    fn new(
        entries: Vec<CgmEntry>,
        thresholds: GlucoseThresholds,
        receiver: mpsc::Receiver<ChartCommand>,
    ) -> Self {
        Self {
            entries,
            thresholds,
            receiver,
        }
    }

    fn close(&self, ctx: &egui::Context) {
        if let Some(state) = CHART_DIALOG.get()
            && let Ok(mut active) = state.lock()
        {
            *active = None;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn poll_commands(&self, ctx: &egui::Context) {
        if matches!(self.receiver.try_recv(), Ok(ChartCommand::Close)) {
            self.close(ctx);
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl Drop for ChartApp {
    fn drop(&mut self) {
        if let Some(state) = CHART_DIALOG.get()
            && let Ok(mut active) = state.lock()
        {
            *active = None;
        }
    }
}

impl eframe::App for ChartApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_commands(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Buffered NightScout Data");
            ui.add_space(8.0);

            if self.entries.is_empty() {
                ui.label("No buffered data points are available yet.");
            } else {
                draw_chart(ui, &self.entries, &self.thresholds);
            }

            ui.add_space(12.0);
            if ui.button("Close").clicked() {
                self.close(ctx);
            }
        });
    }
}

fn draw_chart(ui: &mut egui::Ui, entries: &[CgmEntry], thresholds: &GlucoseThresholds) {
    let desired_size = egui::vec2(ui.available_width().max(320.0), 270.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(110)),
        egui::StrokeKind::Outside,
    );

    let mut points = entries.to_vec();
    points.reverse();

    let mut min = points.iter().map(|entry| entry.sgv).min().unwrap_or(0) as f32;
    let mut max = points.iter().map(|entry| entry.sgv).max().unwrap_or(0) as f32;
    if (max - min).abs() < f32::EPSILON {
        min -= 10.0;
        max += 10.0;
    } else {
        let padding = ((max - min) * 0.1).max(10.0);
        min -= padding;
        max += padding;
    }

    let plot = rect.shrink2(egui::vec2(12.0, 12.0));
    let point_count = points.len();

    let to_screen = |index: usize, reading: u16| -> egui::Pos2 {
        let x = if point_count <= 1 {
            plot.center().x
        } else {
            egui::lerp(
                plot.left()..=plot.right(),
                index as f32 / (point_count as f32 - 1.0),
            )
        };
        let normalized = ((reading as f32 - min) / (max - min)).clamp(0.0, 1.0);
        let y = egui::lerp(plot.bottom()..=plot.top(), normalized);
        egui::pos2(x, y)
    };

    for threshold in [
        thresholds.low_critical,
        thresholds.low_warn,
        thresholds.high_warn,
        thresholds.high_critical,
    ] {
        if (min..=max).contains(&(threshold as f32)) {
            let y = egui::lerp(
                plot.bottom()..=plot.top(),
                (threshold as f32 - min) / (max - min),
            );
            painter.line_segment(
                [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                egui::Stroke::new(
                    1.0,
                    color32_for_reading(threshold, thresholds).gamma_multiply(0.35),
                ),
            );
        }
    }

    for (index, pair) in points.windows(2).enumerate() {
        let start = to_screen(index, pair[0].sgv);
        let end = to_screen(index + 1, pair[1].sgv);
        painter.line_segment(
            [start, end],
            egui::Stroke::new(2.5, color32_for_reading(pair[1].sgv, thresholds)),
        );
    }

    for (index, entry) in points.iter().enumerate() {
        let point = to_screen(index, entry.sgv);
        painter.circle_filled(point, 3.5, color32_for_reading(entry.sgv, thresholds));
    }

    if let Some(pointer) = response.hover_pos() {
        let nearest = points
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry, to_screen(index, entry.sgv)))
            .map(|(entry, point)| (entry, point, point.distance(pointer)))
            .filter(|(_, _, distance)| *distance <= 10.0)
            .min_by(|left, right| left.2.total_cmp(&right.2));

        if let Some((entry, _, _)) = nearest {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new("nightscout-chart-tooltip"),
                egui::PopupAnchor::Pointer,
            )
            .gap(12.0)
            .show(|ui| {
                ui.label(format!("SGV: {} mg/dL", entry.sgv));
                ui.label(
                    entry
                        .date_string
                        .as_deref()
                        .unwrap_or("Timestamp unavailable"),
                );
            });
        }
    }

    let label_color = egui::Color32::from_gray(180);
    painter.text(
        egui::pos2(plot.left(), plot.top()),
        egui::Align2::LEFT_TOP,
        format!("{max:.0}"),
        egui::FontId::monospace(11.0),
        label_color,
    );
    painter.text(
        egui::pos2(plot.left(), plot.bottom()),
        egui::Align2::LEFT_BOTTOM,
        format!("{min:.0}"),
        egui::FontId::monospace(11.0),
        label_color,
    );
    painter.text(
        egui::pos2(plot.left(), plot.bottom() + 10.0),
        egui::Align2::LEFT_TOP,
        "Oldest",
        egui::FontId::monospace(11.0),
        label_color,
    );
    painter.text(
        egui::pos2(plot.right(), plot.bottom() + 10.0),
        egui::Align2::RIGHT_TOP,
        "Newest",
        egui::FontId::monospace(11.0),
        label_color,
    );
}

fn color32_for_reading(reading: u16, thresholds: &GlucoseThresholds) -> egui::Color32 {
    let [r, g, b, a] = color_for_reading(reading, thresholds);
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}
