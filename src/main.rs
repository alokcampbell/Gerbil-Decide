#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use rand::Rng;
use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Gerbil Decide",
        options,
        Box::new(|_cc| Ok(Box::new(WheelApp::load()))),
    )
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Item {
    name: String,
    #[serde(default = "default_weight")]
    weight: u32,
}

fn default_weight() -> u32 {
    1
}

impl Item {
    fn new(name: String) -> Self {
        Self { name, weight: 1 }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]

// probably a better way to do this but oh well for now
struct WheelData {
    name: String,
    items: Vec<Item>,
    #[serde(default)]
    removed_items: Vec<Item>,
    #[serde(default)]
    winner_history: Vec<String>,
    #[serde(default)]
    remove_winner: bool,
    #[serde(default)]
    auto_spin: bool,
}

#[derive(Default)]
struct WheelState {
    input_text: String,
    is_spinning: bool,
    rotation: f32,
    velocity: f32,
    has_stopped: bool,
    stop_delay: f32,
    editing_idx: Option<usize>,
    edit_buf: String,
    pct_bufs: Vec<String>,
}

struct Wheel {
    data: WheelData,
    state: WheelState,
}

// wheel items and data

impl Wheel {
    fn new(name: String) -> Self {
        let items = vec![
            Item::new("gerbil".to_string()),
            Item::new("buxley boys".to_string()),
            Item::new("gerbilamania".to_string()),
            Item::new("gorpen time".to_string()),
            Item::new("how to use eframe".to_string()),
            Item::new("morpen time".to_string()),
        ];
        let pct_bufs = vec![String::new(); items.len()];
        Self {
            data: WheelData {
                name,
                items,
                removed_items: Vec::new(),
                winner_history: Vec::new(),
                remove_winner: false,
                auto_spin: false,
            },
            state: WheelState { pct_bufs, ..Default::default() },
        }
    }

    fn from_data(data: WheelData) -> Self {
        let pct_bufs = vec![String::new(); data.items.len()];
        Self {
            state: WheelState { pct_bufs, ..Default::default() },
            data,
        }
    }

    fn total_weight(&self) -> u32 {
        self.data.items.iter().map(|i| i.weight).sum::<u32>().max(1)
    }

    fn sync_pct_bufs(&mut self) {
        let n = self.data.items.len();
        self.state.pct_bufs.resize(n, String::new());
    }

    fn apply_pct_input(&mut self, idx: usize) -> bool {
        let s = self.state.pct_bufs[idx].trim().trim_end_matches('%').to_string();
        let Ok(pct) = s.parse::<f32>() else { return false };

        let n = self.data.items.len() as f32;
        let clamped = pct.clamp(1.0, (100.0 - (n - 1.0)).max(1.0));

        let others_weight = self.data.items.iter().enumerate()
            .filter(|(i, _)| *i != idx)
            .map(|(_, it)| it.weight)
            .sum::<u32>()
            .max(1);

        self.data.items[idx].weight = (((clamped / (100.0 - clamped)) * others_weight as f32).round() as u32).max(1);
        true
    }

    fn spin(&mut self) {
        self.state.velocity = rand::thread_rng().gen_range(0.5..0.8);
        self.state.rotation = 0.0;
        self.state.is_spinning = true;
        self.state.has_stopped = false;
        self.state.stop_delay = 0.0;
        self.state.editing_idx = None;
    }

    fn tick(&mut self, dt: f32) -> bool {
        if !self.state.is_spinning {
            return false;
        }

        if !self.state.has_stopped {
            self.state.rotation += self.state.velocity;
            self.state.velocity *= 0.975;
            if self.state.velocity < 0.001 {
                self.state.has_stopped = true;
            }
        } else {
            self.state.stop_delay += dt;
            if self.state.stop_delay >= 1.0 {
                self.state.is_spinning = false;
                if !self.data.items.is_empty() {
                    let idx = self.get_winner();
                    let winner = self.data.items[idx].name.clone();
                    self.data.winner_history.insert(0, winner);
                    if self.data.remove_winner {
                        self.data.removed_items.push(self.data.items.remove(idx));
                        self.state.pct_bufs.remove(idx);
                    }
                    if self.data.auto_spin && self.data.remove_winner && self.data.items.len() > 1 {
                        self.spin();
                    }
                    return true;
                }
            }
        }

        false
    }
    fn get_winner(&self) -> usize {
        if self.data.items.is_empty() {
            return 0;
        }
        let total = self.total_weight() as f32;
        let normalized = ((-PI / 2.0 + self.state.rotation) % (2.0 * PI) + 2.0 * PI) % (2.0 * PI);
        let fraction = normalized / (2.0 * PI);
        let mut cumulative = 0.0_f32;
        for (i, item) in self.data.items.iter().enumerate() {
            cumulative += item.weight as f32 / total;
            if fraction < cumulative {
                return i;
            }
        }
        self.data.items.len() - 1
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SaveData {
    wheels: Vec<WheelData>,
    current: usize,
}

struct WheelApp {
    wheels: Vec<Wheel>,
    current: usize,
    show_history: bool,
    show_removed: bool,
    last_time: std::time::Instant,
    needs_save: bool,
}

// save / load data here
impl WheelApp {
    fn load() -> Self {
        let path = Self::save_path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(save) = serde_json::from_str::<SaveData>(&data) {
                let current = save.current.min(save.wheels.len().saturating_sub(1));
                return Self {
                    wheels: save.wheels.into_iter().map(Wheel::from_data).collect(),
                    current,
                    show_history: false,
                    show_removed: false,
                    last_time: std::time::Instant::now(),
                    needs_save: false,
                };
            }
        }
        Self {
            wheels: vec![Wheel::new("Wheel 1".to_string())],
            current: 0,
            show_history: false,
            show_removed: false,
            last_time: std::time::Instant::now(),
            needs_save: false,
        }
    }

    fn save_data(&self) {
        let data = SaveData {
            wheels: self.wheels.iter().map(|w| w.data.clone()).collect(),
            current: self.current,
        };
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let path = Self::save_path();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, json);
        }
    }

    fn save_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("wheel-picker");
        path.push("wheels.json");
        path
    }
}
// eframe lol, this is where all of the actual UI is
impl eframe::App for WheelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f32();
        self.last_time = now;

        if self.wheels[self.current].tick(dt) {
            self.needs_save = true;
        }
        if self.wheels[self.current].state.is_spinning {
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Gerbil Decide");
                ui.separator();

                let mut switch = None;
                for (i, w) in self.wheels.iter().enumerate() {
                    if ui.selectable_label(self.current == i, &w.data.name).clicked() {
                        switch = Some(i);
                    }
                }
                if let Some(i) = switch {
                    self.current = i;
                }

                ui.separator();

                if ui.button("➕ New Wheel").clicked() {
                    let name = format!("Wheel {}", self.wheels.len() + 1);
                    self.wheels.push(Wheel::new(name));
                    self.current = self.wheels.len() - 1;
                    self.needs_save = true;
                }
                if self.wheels.len() > 1 && ui.button("🗑 Delete Wheel").clicked() {
                    self.wheels.remove(self.current);
                    if self.current >= self.wheels.len() {
                        self.current = self.wheels.len() - 1;
                    }
                    self.needs_save = true;
                }
            });
        });

        let mut changed = false;

        egui::SidePanel::left("panel").min_width(260.0).max_width(370.0).show(ctx, |ui| {
            let w = &mut self.wheels[self.current];
            w.sync_pct_bufs();

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("Wheel Name:");
                if ui.text_edit_singleline(&mut w.data.name).changed() {
                    changed = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.heading("Add Items");
            ui.horizontal(|ui| {
                let resp = ui.text_edit_singleline(&mut w.state.input_text);
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (enter || ui.button("Add").clicked()) && !w.state.input_text.trim().is_empty() {
                    let avg_weight = if w.data.items.is_empty() {
                        1
                    } else {
                        (w.total_weight() / w.data.items.len() as u32).max(1)
                    };
                    let mut new_item = Item::new(w.state.input_text.trim().to_string());
                    new_item.weight = avg_weight;
                    w.data.items.push(new_item);
                    w.state.pct_bufs.iter_mut().for_each(|b| b.clear());
                    w.state.pct_bufs.push(String::new());
                    w.state.input_text.clear();
                    changed = true;
                }
            });

            ui.add_space(10.0);
            ui.heading(format!("Items ({})", w.data.items.len()));

            egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                let mut remove_temp: Option<usize> = None;
                let mut remove_perm: Option<usize> = None;
                let mut commit_edit = false;
                let mut apply_pct: Option<usize> = None;
                let total = w.total_weight();

                for i in 0..w.data.items.len() {
                    let pct = w.data.items[i].weight as f32 / total as f32 * 100.0;

                    if w.state.pct_bufs[i].is_empty() {
                        w.state.pct_bufs[i] = format!("{:.0}", pct.round());
                    }

                    ui.horizontal(|ui| {
                        if w.state.editing_idx == Some(i) {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut w.state.edit_buf).desired_width(80.0)
                            );
                            if resp.lost_focus() || ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                                commit_edit = true;
                            }
                            resp.request_focus();
                        } else {
                            let label = ui.add(
                                egui::Label::new(&w.data.items[i].name).sense(egui::Sense::click())
                            );
                            if label.double_clicked() {
                                w.state.editing_idx = Some(i);
                                w.state.edit_buf = w.data.items[i].name.clone();
                            }
                            label.on_hover_text("Double-click to rename");
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Delete forever").clicked() {
                                remove_perm = Some(i);
                            }
                            if ui.small_button("❌").on_hover_text("Remove temporarily").clicked() {
                                remove_temp = Some(i);
                            }

                            ui.label("%");

                            let pct_resp = ui.add(
                                egui::TextEdit::singleline(&mut w.state.pct_bufs[i])
                                    .desired_width(36.0)
                                    .horizontal_align(egui::Align::RIGHT)
                            );
                            if pct_resp.lost_focus() || ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                                apply_pct = Some(i);
                            }
                            if pct_resp.gained_focus() {
                                w.state.pct_bufs[i] = format!("{:.0}", pct.round());
                            }
                        });
                    });
                }

                if let Some(idx) = apply_pct {
                    if w.apply_pct_input(idx) {
                        for b in w.state.pct_bufs.iter_mut() { b.clear(); }
                        changed = true;
                    }
                }

                if commit_edit {
                    if let Some(idx) = w.state.editing_idx {
                        let trimmed = w.state.edit_buf.trim().to_string();
                        if !trimmed.is_empty() {
                            w.data.items[idx].name = trimmed;
                            changed = true;
                        }
                    }
                    w.state.editing_idx = None;
                    w.state.edit_buf.clear();
                }

                if let Some(i) = remove_perm {
                    if w.state.editing_idx == Some(i) { w.state.editing_idx = None; }
                    w.data.items.remove(i);
                    w.state.pct_bufs.remove(i);
                    w.state.pct_bufs.iter_mut().for_each(|b| b.clear());
                    changed = true;
                }
                if let Some(i) = remove_temp {
                    if w.state.editing_idx == Some(i) { w.state.editing_idx = None; }
                    let item = w.data.items.remove(i);
                    w.state.pct_bufs.remove(i);
                    w.state.pct_bufs.iter_mut().for_each(|b| b.clear());
                    w.data.removed_items.push(item);
                    changed = true;
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can_spin = !w.state.is_spinning && w.data.items.len() >= 2;
                if ui.add_enabled(can_spin, egui::Button::new("🎲 SPIN!")).clicked() {
                    w.spin();
                }
                if ui.button("Clear All").clicked() {
                    w.data.items.clear();
                    w.data.winner_history.clear();
                    w.state.pct_bufs.clear();
                    w.state.editing_idx = None;
                    changed = true;
                }
            });

            ui.add_space(5.0);
            if ui.checkbox(&mut w.data.remove_winner, "Remove winner after spin").changed() {
                changed = true;
            }
            if ui.checkbox(&mut w.data.auto_spin, "Keep spinning until one left").changed() {
                changed = true;
            }

            ui.add_space(5.0);

            if !w.data.removed_items.is_empty() {
                ui.separator();
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.heading(format!("Removed ({})", w.data.removed_items.len()));
                    let arrow = if self.show_removed { "▼" } else { "▶" };
                    if ui.small_button(arrow).clicked() {
                        self.show_removed = !self.show_removed;
                    }
                });
                if self.show_removed {
                    egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                        for item in &w.data.removed_items {
                            ui.label(&item.name);
                        }
                    });
                }
                if ui.button("Restore All").clicked() {
                    let count = w.data.removed_items.len();
                    w.data.items.append(&mut w.data.removed_items);
                    for _ in 0..count { w.state.pct_bufs.push(String::new()); }
                    w.state.pct_bufs.iter_mut().for_each(|b| b.clear());
                    changed = true;
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.heading("Winner History");
                let arrow = if self.show_history { "▼" } else { "▶" };
                if ui.small_button(arrow).clicked() {
                    self.show_history = !self.show_history;
                }
            });

            if self.show_history && !w.data.winner_history.is_empty() {
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    for (i, winner) in w.data.winner_history.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}.", i + 1));
                            let color = if i == 0 {
                                egui::Color32::from_rgb(255, 215, 0)
                            } else {
                                egui::Color32::LIGHT_GRAY
                            };
                            ui.label(egui::RichText::new(winner).color(color));
                        });
                    }
                });
                if ui.button("Clear History").clicked() {
                    w.data.winner_history.clear();
                    changed = true;
                }
            }
        });

        if changed || self.needs_save {
            self.save_data();
            self.needs_save = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let w = &self.wheels[self.current];

            if !w.data.winner_history.is_empty() {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.heading("🎉 Latest Winner:");
                    ui.label(
                        egui::RichText::new(&w.data.winner_history[0])
                            .size(36.0)
                            .color(egui::Color32::from_rgb(255, 215, 0)),
                    );
                });
                ui.add_space(10.0);
            }

            ui.separator();
            ui.add_space(20.0);
            // wheel graphics below, i'm not using a png
            if !w.data.items.is_empty() {
                let avail = ui.available_size();
                let size = (avail.y.min(avail.x) * 0.85).max(200.0);
                let total_weight = w.total_weight() as f32;

                ui.vertical_centered(|ui| {
                    let (_id, rect) = ui.allocate_space(egui::vec2(size, size));

                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();
                        let center = rect.center();
                        let radius = size / 2.0 - 10.0;

                        if w.data.items.len() == 1 {
                            painter.circle_filled(center, radius, egui::Color32::from_rgb(100, 150, 200));
                            painter.circle_stroke(center, radius, egui::Stroke::new(2.0, egui::Color32::WHITE));
                            let txt_size = (size / 25.0).max(12.0).min(18.0);
                            painter.text(
                                egui::pos2(center.x, center.y - radius * 0.3),
                                egui::Align2::CENTER_CENTER,
                                &w.data.items[0].name,
                                egui::FontId::proportional(txt_size),
                                egui::Color32::WHITE,
                            );
                        } else {
                            let mut angle_offset = -w.state.rotation;
                            for (i, item) in w.data.items.iter().enumerate() {
                                let slice = 2.0 * PI * (item.weight as f32 / total_weight);
                                let start = angle_offset;
                                let end = angle_offset + slice;

                                let hue = i as f32 / w.data.items.len() as f32;
                                let r = (255.0 * (hue * 6.0).sin().abs()) as u8;
                                let g = (255.0 * ((hue * 6.0) + 2.0).sin().abs()) as u8;
                                let b = (255.0 * ((hue * 6.0) + 4.0).sin().abs()) as u8;

                                let mut pts = vec![center];
                                for s in 0..=30 {
                                    let a = start + (end - start) * s as f32 / 30.0;
                                    pts.push(egui::pos2(
                                        center.x + radius * a.cos(),
                                        center.y + radius * a.sin(),
                                    ));
                                }

                                painter.add(egui::Shape::convex_polygon(
                                    pts,
                                    egui::Color32::from_rgb(r, g, b),
                                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                                ));

                                let mid = (start + end) / 2.0;
                                let txt_r = radius * 0.7;
                                let txt_size = (size / 25.0).max(12.0).min(18.0);
                                painter.text(
                                    egui::pos2(center.x + txt_r * mid.cos(), center.y + txt_r * mid.sin()),
                                    egui::Align2::CENTER_CENTER,
                                    &item.name,
                                    egui::FontId::proportional(txt_size),
                                    egui::Color32::WHITE,
                                );

                                angle_offset = end;
                            }
                        }

                        let dot = (size / 20.0).max(10.0);
                        painter.circle_filled(center, dot, egui::Color32::from_rgb(50, 50, 50));

                        let arrow = size / 50.0;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(center.x, rect.top() + arrow * 2.5),
                                egui::pos2(center.x - arrow, rect.top() + 5.0),
                                egui::pos2(center.x + arrow, rect.top() + 5.0),
                            ],
                            egui::Color32::RED,
                            egui::Stroke::new(2.0, egui::Color32::DARK_RED),
                        ));
                    }
                });
            }
        });
    }
}