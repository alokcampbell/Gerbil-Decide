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
    fn new(wheel_name: String) -> Self {
        let starting_items = vec![
            Item::new("gerbil".to_string()),
            Item::new("buxley boys".to_string()),
            Item::new("gerbilamania".to_string()),
            Item::new("gorpen time".to_string()),
            Item::new("how to use eframe".to_string()),
            Item::new("morpen time".to_string()),
        ];
        let number_of_items = starting_items.len();
        let mut empty_pct_bufs = Vec::new();
        for _ in 0..number_of_items {
            empty_pct_bufs.push(String::new());
        }
        Self {
            data: WheelData {
                name: wheel_name,
                items: starting_items,
                removed_items: Vec::new(),
                winner_history: Vec::new(),
                remove_winner: false,
                auto_spin: false,
            },
            state: WheelState { pct_bufs: empty_pct_bufs, ..Default::default() },
        }
    }

    fn from_data(wheel_data: WheelData) -> Self {
        let number_of_items = wheel_data.items.len();
        let mut empty_pct_bufs = Vec::new();
        for _ in 0..number_of_items {
            empty_pct_bufs.push(String::new());
        }
        Self {
            state: WheelState { pct_bufs: empty_pct_bufs, ..Default::default() },
            data: wheel_data,
        }
    }

    fn total_weight(&self) -> u32 {
        let mut total = 0;
        for item in &self.data.items {
            total += item.weight;
        }
        if total == 0 {
            return 1;
        }
        total
    }

    fn sync_pct_bufs(&mut self) {
        let number_of_items = self.data.items.len();
        self.state.pct_bufs.resize(number_of_items, String::new());
    }

    fn apply_pct_input(&mut self, item_index: usize) -> bool {
        let raw_input = self.state.pct_bufs[item_index].trim().trim_end_matches('%').to_string();
        let parsed = raw_input.parse::<f32>();
        let pct = match parsed {
            Ok(value) => value,
            Err(_) => return false,
        };

        let number_of_items = self.data.items.len() as f32;
        let min_pct = 1.0_f32;
        let max_pct = (100.0 - (number_of_items - 1.0)).max(1.0);
        let clamped_pct = pct.clamp(min_pct, max_pct);

        let mut others_total_weight = 0_u32;
        for (index, item) in self.data.items.iter().enumerate() {
            if index != item_index {
                others_total_weight += item.weight;
            }
        }
        if others_total_weight == 0 {
            others_total_weight = 1;
        }

        let new_weight = ((clamped_pct / (100.0 - clamped_pct)) * others_total_weight as f32).round() as u32;
        self.data.items[item_index].weight = new_weight.max(1);
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
                    let winning_index = self.get_winner();
                    let winning_name = self.data.items[winning_index].name.clone();
                    self.data.winner_history.insert(0, winning_name);
                    if self.data.remove_winner {
                        let removed_item = self.data.items.remove(winning_index);
                        self.data.removed_items.push(removed_item);
                        self.state.pct_bufs.remove(winning_index);
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
        let total_weight = self.total_weight() as f32;
        let normalized_angle = ((-PI / 2.0 + self.state.rotation) % (2.0 * PI) + 2.0 * PI) % (2.0 * PI);
        let fraction_of_circle = normalized_angle / (2.0 * PI);
        let mut cumulative_fraction = 0.0_f32;
        for (index, item) in self.data.items.iter().enumerate() {
            cumulative_fraction += item.weight as f32 / total_weight;
            if fraction_of_circle < cumulative_fraction {
                return index;
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
        let save_file_path = Self::save_path();
        if let Ok(file_contents) = fs::read_to_string(&save_file_path) {
            if let Ok(save_data) = serde_json::from_str::<SaveData>(&file_contents) {
                let current_wheel_index = save_data.current.min(save_data.wheels.len().saturating_sub(1));
                let loaded_wheels: Vec<Wheel> = save_data.wheels.into_iter().map(Wheel::from_data).collect();
                return Self {
                    wheels: loaded_wheels,
                    current: current_wheel_index,
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
        let mut all_wheel_data = Vec::new();
        for wheel in &self.wheels {
            all_wheel_data.push(wheel.data.clone());
        }
        let save_data = SaveData {
            wheels: all_wheel_data,
            current: self.current,
        };
        if let Ok(json_string) = serde_json::to_string_pretty(&save_data) {
            let save_file_path = Self::save_path();
            if let Some(parent_folder) = save_file_path.parent() {
                let _ = fs::create_dir_all(parent_folder);
            }
            let _ = fs::write(save_file_path, json_string);
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
        let current_time = std::time::Instant::now();
        let dt = current_time.duration_since(self.last_time).as_secs_f32();
        self.last_time = current_time;

        let spin_just_finished = self.wheels[self.current].tick(dt);
        if spin_just_finished {
            self.needs_save = true;
        }
        if self.wheels[self.current].state.is_spinning {
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Gerbil Decide");
                ui.separator();

                let mut switch_to_wheel = None;
                for (wheel_index, wheel) in self.wheels.iter().enumerate() {
                    let is_selected = self.current == wheel_index;
                    if ui.selectable_label(is_selected, &wheel.data.name).clicked() {
                        switch_to_wheel = Some(wheel_index);
                    }
                }
                if let Some(wheel_index) = switch_to_wheel {
                    self.current = wheel_index;
                }

                ui.separator();

                if ui.button("➕ New Wheel").clicked() {
                    let new_wheel_name = format!("Wheel {}", self.wheels.len() + 1);
                    self.wheels.push(Wheel::new(new_wheel_name));
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

        let mut something_changed = false;

        egui::SidePanel::left("panel").min_width(260.0).max_width(370.0).show(ctx, |ui| {
            let current_wheel = &mut self.wheels[self.current];
            current_wheel.sync_pct_bufs();

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("Wheel Name:");
                if ui.text_edit_singleline(&mut current_wheel.data.name).changed() {
                    something_changed = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.heading("Add Items");
            ui.horizontal(|ui| {
                let text_box_response = ui.text_edit_singleline(&mut current_wheel.state.input_text);
                let pressed_enter = text_box_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked_add = ui.button("Add").clicked();
                let has_text = !current_wheel.state.input_text.trim().is_empty();

                if (pressed_enter || clicked_add) && has_text {
                    let avg_weight = if current_wheel.data.items.is_empty() {
                        1
                    } else {
                        (current_wheel.total_weight() / current_wheel.data.items.len() as u32).max(1)
                    };
                    let new_item_name = current_wheel.state.input_text.trim().to_string();
                    let mut new_item = Item::new(new_item_name);
                    new_item.weight = avg_weight;
                    current_wheel.data.items.push(new_item);
                    for buf in current_wheel.state.pct_bufs.iter_mut() {
                        buf.clear();
                    }
                    current_wheel.state.pct_bufs.push(String::new());
                    current_wheel.state.input_text.clear();
                    something_changed = true;
                }
            });

            ui.add_space(10.0);
            ui.heading(format!("Items ({})", current_wheel.data.items.len()));

            egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                let mut remove_temp: Option<usize> = None;
                let mut remove_perm: Option<usize> = None;
                let mut should_commit_edit = false;
                let mut apply_pct_for_index: Option<usize> = None;
                let total_weight = current_wheel.total_weight();

                for item_index in 0..current_wheel.data.items.len() {
                    let item_pct = current_wheel.data.items[item_index].weight as f32 / total_weight as f32 * 100.0;

                    if current_wheel.state.pct_bufs[item_index].is_empty() {
                        current_wheel.state.pct_bufs[item_index] = format!("{:.0}", item_pct.round());
                    }

                    ui.horizontal(|ui| {
                        let currently_editing_this_item = current_wheel.state.editing_idx == Some(item_index);
                        if currently_editing_this_item {
                            let edit_response = ui.add(
                                egui::TextEdit::singleline(&mut current_wheel.state.edit_buf).desired_width(80.0)
                            );
                            let pressed_enter = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                            if edit_response.lost_focus() || pressed_enter {
                                should_commit_edit = true;
                            }
                            edit_response.request_focus();
                        } else {
                            let item_label = ui.add(
                                egui::Label::new(&current_wheel.data.items[item_index].name).sense(egui::Sense::click())
                            );
                            if item_label.double_clicked() {
                                current_wheel.state.editing_idx = Some(item_index);
                                current_wheel.state.edit_buf = current_wheel.data.items[item_index].name.clone();
                            }
                            item_label.on_hover_text("Double-click to rename");
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Delete forever").clicked() {
                                remove_perm = Some(item_index);
                            }
                            if ui.small_button("❌").on_hover_text("Remove temporarily").clicked() {
                                remove_temp = Some(item_index);
                            }

                            ui.label("%");

                            let pct_box_response = ui.add(
                                egui::TextEdit::singleline(&mut current_wheel.state.pct_bufs[item_index])
                                    .desired_width(36.0)
                                    .horizontal_align(egui::Align::RIGHT)
                            );
                            let pressed_enter = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                            if pct_box_response.lost_focus() || pressed_enter {
                                apply_pct_for_index = Some(item_index);
                            }
                            if pct_box_response.gained_focus() {
                                current_wheel.state.pct_bufs[item_index] = format!("{:.0}", item_pct.round());
                            }
                        });
                    });
                }

                if let Some(item_index) = apply_pct_for_index {
                    let did_apply = current_wheel.apply_pct_input(item_index);
                    if did_apply {
                        for buf in current_wheel.state.pct_bufs.iter_mut() {
                            buf.clear();
                        }
                        something_changed = true;
                    }
                }

                if should_commit_edit {
                    if let Some(editing_index) = current_wheel.state.editing_idx {
                        let new_name = current_wheel.state.edit_buf.trim().to_string();
                        if !new_name.is_empty() {
                            current_wheel.data.items[editing_index].name = new_name;
                            something_changed = true;
                        }
                    }
                    current_wheel.state.editing_idx = None;
                    current_wheel.state.edit_buf.clear();
                }

                if let Some(item_index) = remove_perm {
                    if current_wheel.state.editing_idx == Some(item_index) {
                        current_wheel.state.editing_idx = None;
                    }
                    current_wheel.data.items.remove(item_index);
                    current_wheel.state.pct_bufs.remove(item_index);
                    for buf in current_wheel.state.pct_bufs.iter_mut() {
                        buf.clear();
                    }
                    something_changed = true;
                }
                if let Some(item_index) = remove_temp {
                    if current_wheel.state.editing_idx == Some(item_index) {
                        current_wheel.state.editing_idx = None;
                    }
                    let moved_item = current_wheel.data.items.remove(item_index);
                    current_wheel.state.pct_bufs.remove(item_index);
                    for buf in current_wheel.state.pct_bufs.iter_mut() {
                        buf.clear();
                    }
                    current_wheel.data.removed_items.push(moved_item);
                    something_changed = true;
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let wheel_has_enough_items = current_wheel.data.items.len() >= 2;
                let can_spin = !current_wheel.state.is_spinning && wheel_has_enough_items;
                if ui.add_enabled(can_spin, egui::Button::new("🎲 SPIN!")).clicked() {
                    current_wheel.spin();
                }
                if ui.button("Clear All").clicked() {
                    current_wheel.data.items.clear();
                    current_wheel.data.winner_history.clear();
                    current_wheel.state.pct_bufs.clear();
                    current_wheel.state.editing_idx = None;
                    something_changed = true;
                }
            });

            ui.add_space(5.0);
            if ui.checkbox(&mut current_wheel.data.remove_winner, "Remove winner after spin").changed() {
                something_changed = true;
            }
            if ui.checkbox(&mut current_wheel.data.auto_spin, "Keep spinning until one left").changed() {
                something_changed = true;
            }

            ui.add_space(5.0);

            if !current_wheel.data.removed_items.is_empty() {
                ui.separator();
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.heading(format!("Removed ({})", current_wheel.data.removed_items.len()));
                    let arrow_symbol = if self.show_removed { "▼" } else { "▶" };
                    if ui.small_button(arrow_symbol).clicked() {
                        self.show_removed = !self.show_removed;
                    }
                });
                if self.show_removed {
                    egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                        for removed_item in &current_wheel.data.removed_items {
                            ui.label(&removed_item.name);
                        }
                    });
                }
                if ui.button("Restore All").clicked() {
                    let how_many_removed = current_wheel.data.removed_items.len();
                    current_wheel.data.items.append(&mut current_wheel.data.removed_items);
                    for _ in 0..how_many_removed {
                        current_wheel.state.pct_bufs.push(String::new());
                    }
                    for buf in current_wheel.state.pct_bufs.iter_mut() {
                        buf.clear();
                    }
                    something_changed = true;
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.heading("Winner History");
                let arrow_symbol = if self.show_history { "▼" } else { "▶" };
                if ui.small_button(arrow_symbol).clicked() {
                    self.show_history = !self.show_history;
                }
            });

            let history_is_visible = self.show_history && !current_wheel.data.winner_history.is_empty();
            if history_is_visible {
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    for (history_index, winner_name) in current_wheel.data.winner_history.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}.", history_index + 1));
                            let text_color = if history_index == 0 {
                                egui::Color32::from_rgb(255, 215, 0)
                            } else {
                                egui::Color32::LIGHT_GRAY
                            };
                            ui.label(egui::RichText::new(winner_name).color(text_color));
                        });
                    }
                });
                if ui.button("Clear History").clicked() {
                    current_wheel.data.winner_history.clear();
                    something_changed = true;
                }
            }
        });

        if something_changed || self.needs_save {
            self.save_data();
            self.needs_save = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let current_wheel = &self.wheels[self.current];

            if !current_wheel.data.winner_history.is_empty() {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.heading("🎉 Latest Winner:");
                    let latest_winner_name = &current_wheel.data.winner_history[0];
                    ui.label(
                        egui::RichText::new(latest_winner_name)
                            .size(36.0)
                            .color(egui::Color32::from_rgb(255, 215, 0)),
                    );
                });
                ui.add_space(10.0);
            }

            ui.separator();
            ui.add_space(20.0);
            // wheel graphics below, i'm not using a png
            if !current_wheel.data.items.is_empty() {
                let available_space = ui.available_size();
                let wheel_size = (available_space.y.min(available_space.x) * 0.85).max(200.0);
                let total_weight = current_wheel.total_weight() as f32;

                ui.vertical_centered(|ui| {
                    let (_id, wheel_rect) = ui.allocate_space(egui::vec2(wheel_size, wheel_size));

                    if ui.is_rect_visible(wheel_rect) {
                        let painter = ui.painter();
                        let wheel_center = wheel_rect.center();
                        let wheel_radius = wheel_size / 2.0 - 10.0;

                        if current_wheel.data.items.len() == 1 {
                            painter.circle_filled(wheel_center, wheel_radius, egui::Color32::from_rgb(100, 150, 200));
                            painter.circle_stroke(wheel_center, wheel_radius, egui::Stroke::new(2.0, egui::Color32::WHITE));
                            let font_size = (wheel_size / 25.0).max(12.0).min(18.0);
                            painter.text(
                                egui::pos2(wheel_center.x, wheel_center.y - wheel_radius * 0.3),
                                egui::Align2::CENTER_CENTER,
                                &current_wheel.data.items[0].name,
                                egui::FontId::proportional(font_size),
                                egui::Color32::WHITE,
                            );
                        } else {
                            let mut current_angle = -current_wheel.state.rotation;
                            for (item_index, item) in current_wheel.data.items.iter().enumerate() {
                                let slice_angle = 2.0 * PI * (item.weight as f32 / total_weight);
                                let slice_start_angle = current_angle;
                                let slice_end_angle = current_angle + slice_angle;

                                let hue = item_index as f32 / current_wheel.data.items.len() as f32;
                                let red_amount = (255.0 * (hue * 6.0).sin().abs()) as u8;
                                let green_amount = (255.0 * ((hue * 6.0) + 2.0).sin().abs()) as u8;
                                let blue_amount = (255.0 * ((hue * 6.0) + 4.0).sin().abs()) as u8;
                                let slice_color = egui::Color32::from_rgb(red_amount, green_amount, blue_amount);

                                let mut slice_points = vec![wheel_center];
                                for step in 0..=30 {
                                    let angle_at_step = slice_start_angle + (slice_end_angle - slice_start_angle) * step as f32 / 30.0;
                                    let point_x = wheel_center.x + wheel_radius * angle_at_step.cos();
                                    let point_y = wheel_center.y + wheel_radius * angle_at_step.sin();
                                    slice_points.push(egui::pos2(point_x, point_y));
                                }

                                painter.add(egui::Shape::convex_polygon(
                                    slice_points,
                                    slice_color,
                                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                                ));

                                let label_angle = (slice_start_angle + slice_end_angle) / 2.0;
                                let label_radius = wheel_radius * 0.7;
                                let label_x = wheel_center.x + label_radius * label_angle.cos();
                                let label_y = wheel_center.y + label_radius * label_angle.sin();
                                let font_size = (wheel_size / 25.0).max(12.0).min(18.0);
                                painter.text(
                                    egui::pos2(label_x, label_y),
                                    egui::Align2::CENTER_CENTER,
                                    &item.name,
                                    egui::FontId::proportional(font_size),
                                    egui::Color32::WHITE,
                                );

                                current_angle = slice_end_angle;
                            }
                        }

                        let center_dot_size = (wheel_size / 20.0).max(10.0);
                        painter.circle_filled(wheel_center, center_dot_size, egui::Color32::from_rgb(50, 50, 50));

                        let arrow_size = wheel_size / 50.0;
                        let arrow_tip_y = wheel_rect.top() + arrow_size * 2.5;
                        let arrow_left_x = wheel_center.x - arrow_size;
                        let arrow_right_x = wheel_center.x + arrow_size;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(wheel_center.x, arrow_tip_y),
                                egui::pos2(arrow_left_x, wheel_rect.top() + 5.0),
                                egui::pos2(arrow_right_x, wheel_rect.top() + 5.0),
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