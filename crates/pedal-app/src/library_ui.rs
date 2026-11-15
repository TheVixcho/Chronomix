use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, UNIX_EPOCH},
};

use eframe::egui::{
    self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2,
};
use pedal_engine::{ChannelInputMode, MixerEngine};

use crate::{
    i18n::Lang,
    mixer_ui::{MixerUiState, WingSelection},
    open_folder_in_file_manager,
    theme::{StudioIcon, StudioPalette, draw_vector_icon},
};

#[derive(Debug, Clone)]
pub struct AudioFileInfo {
    pub path: PathBuf,
    pub filename: String,
    pub parent_folder: String,
    pub size_bytes: u64,
    pub modified_secs: u64,
    pub extension: String,
}

pub struct LibraryUiState {
    pub scan_dir: PathBuf,
    pub files: Vec<AudioFileInfo>,
    pub selected_idx: Option<usize>,
    pub search_query: String,
    pub last_scan: Instant,
    pub assign_target_ch: usize,
    pub status_notice: Option<String>,
}

impl LibraryUiState {
    pub fn new(initial_dir: PathBuf) -> Self {
        let mut state = Self {
            scan_dir: initial_dir,
            files: Vec::new(),
            selected_idx: None,
            search_query: String::new(),
            last_scan: Instant::now(),
            assign_target_ch: 0,
            status_notice: None,
        };
        state.refresh_scan();
        state
    }

    pub fn refresh_scan(&mut self) {
        let mut collected = Vec::new();
        scan_audio_files_recursive(&self.scan_dir, 0, 3, &mut collected);
        collected.sort_by(|a, b| b.modified_secs.cmp(&a.modified_secs));
        self.files = collected;
        self.last_scan = Instant::now();
        if let Some(sel) = self.selected_idx {
            if sel >= self.files.len() {
                self.selected_idx = self.files.first().map(|_| 0);
            }
        }
    }
}

fn scan_audio_files_recursive(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<AudioFileInfo>) {
    if depth > max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                scan_audio_files_recursive(&path, depth + 1, max_depth, out);
            }
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(ext.as_str(), "wav" | "flac" | "mp3" | "ogg" | "aiff" | "m4a") {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("audio")
                    .to_string();

                let parent_folder = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                let metadata = fs::metadata(&path).ok();
                let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified_secs = metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                out.push(AudioFileInfo {
                    path,
                    filename,
                    parent_folder,
                    size_bytes,
                    modified_secs,
                    extension: ext.to_ascii_uppercase(),
                });
            }
        }
    }
}

pub struct LibraryPanelContext<'a> {
    pub lang: Lang,
    pub state: &'a mut LibraryUiState,
    pub mixer: &'a mut MixerEngine,
    pub mixer_ui: &'a mut MixerUiState,
    pub fallback_output: Option<&'a str>,
    pub switch_to_mixer: &'a mut bool,
}

pub fn render_library_panel(ui: &mut egui::Ui, mut ctx: LibraryPanelContext<'_>) {
    egui::Frame::new()
        .fill(StudioPalette::BG_DEEP)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            render_library_top_bar(ui, &mut ctx);
            ui.add_space(8.0);

            let available_height = ui.available_height().max(200.0);
            ui.columns(2, |cols| {
                cols[0].set_min_height(available_height);
                cols[1].set_min_height(available_height);

                render_file_list(&mut cols[0], &mut ctx);
                render_detail_and_assign(&mut cols[1], &mut ctx);
            });
        });
}

fn render_library_top_bar(ui: &mut egui::Ui, ctx: &mut LibraryPanelContext<'_>) {
    egui::Frame::new()
        .fill(Color32::from_rgb(18, 19, 23))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 40, 48)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon_r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                draw_vector_icon(ui.painter(), icon_r.center(), 14.0, StudioIcon::Folder, Color32::from_rgb(90, 180, 255));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("BIBLIOTECA DE AUDIO & GRABACIONES")
                        .strong()
                        .color(StudioPalette::TEXT_PRIMARY)
                        .size(13.0),
                );

                ui.add_space(12.0);
                ui.label(
                    RichText::new(ctx.state.scan_dir.display().to_string())
                        .monospace()
                        .color(StudioPalette::TEXT_MUTED)
                        .size(11.0),
                );

                if ui.button(RichText::new("Cambiar Carpeta...").color(StudioPalette::TEXT_PRIMARY)).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().set_directory(&ctx.state.scan_dir).pick_folder() {
                        ctx.state.scan_dir = folder;
                        ctx.state.refresh_scan();
                    }
                }

                if ui.button(RichText::new("Abrir Carpeta").color(StudioPalette::TEXT_SECONDARY)).clicked() {
                    let _ = open_folder_in_file_manager(&ctx.state.scan_dir);
                }

                if ui.button(RichText::new("Actualizar").color(Color32::from_rgb(90, 180, 255))).clicked() {
                    ctx.state.refresh_scan();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let count = ctx.state.files.len();
                    ui.label(
                        RichText::new(format!("{count} archivos de audio"))
                            .small()
                            .color(Color32::from_rgb(160, 165, 175)),
                    );
                });
            });
        });
}

fn render_file_list(ui: &mut egui::Ui, ctx: &mut LibraryPanelContext<'_>) {
    egui::Frame::new()
        .fill(Color32::from_rgb(22, 23, 27))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 46, 54)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Buscar:").small().color(StudioPalette::TEXT_MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut ctx.state.search_query)
                        .hint_text("Filtrar archivos por nombre o sesión...")
                        .desired_width(f32::INFINITY),
                );
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            let query = ctx.state.search_query.trim().to_ascii_lowercase();
            let mut clicked_idx = None;

            egui::ScrollArea::vertical()
                .id_salt("library_files_scroll")
                .show(ui, |ui| {
                    if ctx.state.files.is_empty() {
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No se encontraron archivos de audio en esta carpeta.")
                                    .color(StudioPalette::TEXT_MUTED),
                            );
                            ui.label(
                                RichText::new("Graba canales con el botón REC del Mixer o cambia la carpeta de escaneo.")
                                    .small()
                                    .color(StudioPalette::TEXT_MUTED),
                            );
                        });
                        return;
                    }

                    for (idx, file) in ctx.state.files.iter().enumerate() {
                        if !query.is_empty()
                            && !file.filename.to_ascii_lowercase().contains(&query)
                            && !file.parent_folder.to_ascii_lowercase().contains(&query)
                        {
                            continue;
                        }

                        let is_selected = ctx.state.selected_idx == Some(idx);
                        let bg = if is_selected {
                            Color32::from_rgb(38, 42, 54)
                        } else {
                            Color32::from_rgb(26, 27, 32)
                        };
                        let border = if is_selected {
                            Color32::from_rgb(90, 180, 255)
                        } else {
                            Color32::from_rgb(40, 42, 50)
                        };

                        let item_frame = egui::Frame::new()
                            .fill(bg)
                            .stroke(Stroke::new(1.0_f32, border))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(8, 6));

                        let resp = item_frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(45, 48, 58))
                                    .corner_radius(CornerRadius::same(3))
                                    .inner_margin(Margin::symmetric(4, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&file.extension)
                                                .monospace()
                                                .size(9.0)
                                                .color(Color32::from_rgb(220, 224, 235)),
                                        );
                                    });

                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&file.filename).strong().color(Color32::WHITE));
                                    if !file.parent_folder.is_empty() {
                                        ui.label(
                                            RichText::new(format!("📁 {}", file.parent_folder))
                                                .small()
                                                .color(StudioPalette::TEXT_MUTED),
                                        );
                                    }
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let mb = file.size_bytes as f64 / (1024.0 * 1024.0);
                                    let size_str = if mb >= 1.0 {
                                        format!("{mb:.2} MB")
                                    } else {
                                        format!("{:.0} KB", file.size_bytes as f64 / 1024.0)
                                    };
                                    ui.label(RichText::new(size_str).small().monospace().color(StudioPalette::TEXT_MUTED));
                                });
                            });
                        });

                        if resp.response.interact(Sense::click()).clicked() {
                            clicked_idx = Some(idx);
                        }
                        ui.add_space(2.0);
                    }
                });

            if let Some(c) = clicked_idx {
                ctx.state.selected_idx = Some(c);
            }
        });
}

fn render_detail_and_assign(ui: &mut egui::Ui, ctx: &mut LibraryPanelContext<'_>) {
    egui::Frame::new()
        .fill(Color32::from_rgb(22, 23, 27))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 46, 54)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let sel_file = ctx
                .state
                .selected_idx
                .and_then(|i| ctx.state.files.get(i).cloned());

            match sel_file {
                None => {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Selecciona un archivo de audio de la lista")
                                .strong()
                                .color(StudioPalette::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new("Podrás pre-escucharlo o asignarlo directamente a un canal Multimedia del Mixer.")
                                .small()
                                .color(StudioPalette::TEXT_MUTED),
                        );
                    });
                }
                Some(file) => {
                    ui.label(RichText::new("DETALLES DEL ARCHIVO").strong().color(Color32::from_rgb(90, 180, 255)));
                    ui.add_space(6.0);

                    egui::Frame::new()
                        .fill(Color32::from_rgb(16, 17, 20))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&file.filename).size(14.0).strong().color(Color32::WHITE));
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(file.path.display().to_string())
                                    .small()
                                    .monospace()
                                    .color(StudioPalette::TEXT_MUTED),
                            );

                            let mb = file.size_bytes as f64 / (1024.0 * 1024.0);
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(format!("Tamaño: {:.2} MB | Formato: {}", mb, file.extension))
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            );
                        });

                    ui.add_space(12.0);
                    ui.label(RichText::new("ASIGNAR A CANAL MULTIMEDIA").strong().color(Color32::from_rgb(240, 160, 120)));
                    ui.add_space(4.0);

                    let channel_count = ctx.mixer.channels().len();
                    let channels: Vec<(usize, String)> = ctx
                        .mixer
                        .channels()
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let is_media = c.source.mode == ChannelInputMode::Multimedia;
                            let tag = if is_media { " [MEDIA]" } else { "" };
                            (i, format!("CH {:02}: {}{tag}", i + 1, c.name))
                        })
                        .collect();

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Destino:").color(StudioPalette::TEXT_SECONDARY));

                        let cur_target = ctx.state.assign_target_ch;
                        let cur_label = if cur_target == 0 {
                            "+ Crear Nuevo Canal Multimedia".to_string()
                        } else if let Some((_, name)) = channels.iter().find(|(idx, _)| *idx + 1 == cur_target) {
                            name.clone()
                        } else {
                            "+ Crear Nuevo Canal Multimedia".to_string()
                        };

                        egui::ComboBox::from_id_salt("library_assign_combo")
                            .selected_text(cur_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut ctx.state.assign_target_ch, 0, "+ Crear Nuevo Canal Multimedia");
                                for (ch_idx, name) in &channels {
                                    ui.selectable_value(&mut ctx.state.assign_target_ch, ch_idx + 1, name);
                                }
                            });
                    });

                    ui.add_space(10.0);
                    let mut assigned_channel = None;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("🎚 Asignar y Cargar en Mixer")
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(45, 140, 90))
                            .corner_radius(CornerRadius::same(5)),
                        )
                        .clicked()
                    {
                        let target_ch = if ctx.state.assign_target_ch == 0 {
                            if let Some(new_ch) = ctx.mixer.add_multimedia_channel() {
                                new_ch
                            } else {
                                channel_count.saturating_sub(1)
                            }
                        } else {
                            ctx.state.assign_target_ch - 1
                        };

                        ctx.mixer.set_channel_multimedia(target_ch);
                        ctx.mixer.request_load_media(target_ch, file.path.clone());
                        ctx.mixer_ui.selection = WingSelection::Channel(target_ch);
                        ctx.state.status_notice = Some(format!(
                            "Cargado '{}' en Canal CH {:02}",
                            file.filename,
                            target_ch + 1
                        ));
                        assigned_channel = Some(target_ch);
                    }

                    if let Some(notice) = &ctx.state.status_notice {
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(Color32::from_rgb(24, 48, 32))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(50, 160, 90)))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.label(RichText::new(notice).strong().color(Color32::from_rgb(180, 240, 190)));
                            });
                    }

                    if assigned_channel.is_some() || ctx.state.status_notice.is_some() {
                        ui.add_space(8.0);
                        if ui
                            .button(RichText::new("Ir a la Mesa de Mezclas ->").strong().color(Color32::from_rgb(90, 180, 255)))
                            .clicked()
                        {
                            *ctx.switch_to_mixer = true;
                        }
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_ui_state_initializes_and_scans() {
        let temp_dir = std::env::temp_dir().join("chronomix_lib_test");
        let _ = fs::create_dir_all(&temp_dir);
        let sample_file = temp_dir.join("test_sample.wav");
        let _ = fs::write(&sample_file, b"RIFFfakeWAVEfmt ");

        let state = LibraryUiState::new(temp_dir.clone());
        assert!(state.files.iter().any(|f| f.filename == "test_sample.wav"));

        let _ = fs::remove_file(&sample_file);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
