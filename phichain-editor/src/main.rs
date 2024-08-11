#[macro_use]
extern crate rust_i18n;

mod action;
mod audio;
mod cli;
mod constants;
mod editing;
mod export;
mod exporter;
mod file;
mod hit_sound;
mod home;
mod hotkey;
mod identifier;
mod loader;
mod misc;
mod notification;
mod project;
mod recent_projects;
mod score;
mod screenshot;
mod selection;
mod settings;
mod tab;
mod timeline;
mod timing;
mod translation;
mod ui;
mod utils;

use crate::action::{ActionPlugin, ActionRegistry};
use crate::audio::AudioPlugin;
use crate::cli::{Args, CliPlugin};
use crate::editing::history::EditorHistory;
use crate::editing::EditingPlugin;
use crate::export::ExportPlugin;
use crate::exporter::phichain::PhichainExporter;
use crate::exporter::Exporter;
use crate::file::{pick_folder, FilePickingPlugin, PickingKind};
use crate::hit_sound::HitSoundPlugin;
use crate::home::HomePlugin;
use crate::hotkey::{HotkeyPlugin, HotkeyRegistrationExt};
use crate::misc::MiscPlugin;
use crate::notification::NotificationPlugin;
use crate::project::project_loaded;
use crate::project::LoadProjectEvent;
use crate::project::ProjectPlugin;
use crate::recent_projects::RecentProjectsPlugin;
use crate::score::ScorePlugin;
use crate::screenshot::ScreenshotPlugin;
use crate::selection::Selected;
use crate::settings::{AspectRatio, EditorSettings, EditorSettingsPlugin};
use crate::tab::game::GameCamera;
use crate::tab::game::GameTabPlugin;
use crate::tab::game::GameViewport;
use crate::tab::quick_action::quick_action;
use crate::tab::timeline::TimelineViewport;
use crate::tab::TabPlugin;
use crate::tab::{EditorTab, TabRegistry};
use crate::timeline::TimelinePlugin;
use crate::timing::TimingPlugin;
use crate::translation::TranslationPlugin;
use crate::ui::UiPlugin;
use crate::utils::compat::ControlKeyExt;
use crate::utils::convert::BevyEguiConvert;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::settings::WgpuSettings;
use bevy::render::RenderPlugin;
use bevy_egui::egui::{Color32, Frame};
use bevy_egui::{EguiContext, EguiPlugin};
use bevy_mod_picking::prelude::*;
use bevy_persistent::Persistent;
use bevy_prototype_lyon::prelude::ShapePlugin;
use egui_dock::{DockArea, DockState, NodeIndex, Style};
use phichain_assets::AssetsPlugin;
use phichain_chart::event::LineEvent;
use phichain_chart::note::Note;
use phichain_game::{GamePlugin, GameSet};
use rfd::FileDialog;
use rust_i18n::set_locale;
use std::env;
use std::path::PathBuf;

i18n!("lang", fallback = "en_us");

fn main() {
    let mut wgpu_settings = WgpuSettings::default();
    wgpu_settings
        .features
        .set(WgpuFeatures::VERTEX_WRITABLE_STORAGE, true);

    #[cfg(debug_assertions)]
    {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.parent().expect("Failed to get root path");
        env::set_var("BEVY_ASSET_ROOT", root);
    }

    App::new()
        .configure_sets(PostUpdate, GameSet.run_if(project_loaded()))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(UiState::new())
        .add_plugins(CliPlugin)
        .add_plugins(MiscPlugin)
        .add_plugins(UiPlugin)
        .add_plugins(TranslationPlugin)
        .add_plugins(RecentProjectsPlugin)
        .add_plugins(HomePlugin)
        .add_plugins(DefaultPlugins.set(RenderPlugin {
            render_creation: wgpu_settings.into(),
            synchronous_pipeline_compilation: false,
        }))
        .add_plugins(ShapePlugin)
        .add_plugins(GamePlugin)
        .add_plugins(ActionPlugin)
        .add_plugins(HotkeyPlugin)
        .add_plugins(ScreenshotPlugin)
        .add_plugins(TimingPlugin)
        .add_plugins(AudioPlugin)
        .add_plugins(EditorSettingsPlugin)
        .add_plugins(HitSoundPlugin)
        .add_plugins(GameTabPlugin)
        .add_plugins(ScorePlugin)
        .add_plugins(TimelinePlugin)
        .add_plugins(DefaultPickingPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(ProjectPlugin)
        .add_plugins(ExportPlugin)
        .add_plugins(selection::SelectionPlugin)
        .add_plugins(TabPlugin)
        .add_plugins(EditingPlugin)
        .add_plugins(FrameTimeDiagnosticsPlugin)
        .add_plugins(AssetsPlugin)
        .add_plugins(NotificationPlugin)
        .add_plugins(FilePickingPlugin)
        .add_systems(Startup, setup_egui_image_loader_system)
        .add_systems(Startup, setup_egui_font_system)
        .add_systems(Startup, setup_plugin)
        .add_systems(Update, ui_system.run_if(project_loaded()))
        .add_systems(Update, debug_save_system.run_if(project_loaded()))
        .add_systems(
            Startup,
            (apply_args_config_system, apply_editor_settings_system),
        )
        .register_hotkey(
            "phichain.project.save",
            vec![KeyCode::control(), KeyCode::KeyS],
        )
        .run();
}

fn debug_save_system(world: &mut World) {
    let event = world.resource::<ButtonInput<KeyCode>>();
    if event.just_pressed(KeyCode::KeyE) {
        if let Ok(chart) = PhichainExporter::export(world) {
            let _ = std::fs::write("Chart.json", chart);
        }
    }
}

fn apply_editor_settings_system(settings: Res<Persistent<EditorSettings>>) {
    set_locale(settings.general.language.as_str());
}

/// Apply configurations from the command line args
fn apply_args_config_system(args: Res<Args>, mut events: EventWriter<LoadProjectEvent>) {
    // load chart if specified
    if let Some(path) = &args.project {
        events.send(LoadProjectEvent(path.into()));
    }
}

fn setup_egui_image_loader_system(mut contexts: bevy_egui::EguiContexts) {
    egui_extras::install_image_loaders(contexts.ctx_mut());
}

fn setup_egui_font_system(mut contexts: bevy_egui::EguiContexts) {
    let ctx = contexts.ctx_mut();

    let font_file = utils::assets::get_base_path()
        .join("assets/font/MiSans-Regular.ttf")
        .to_str()
        .unwrap()
        .to_string();
    let font_name = "MiSans-Regular".to_string();
    let font_file_bytes = std::fs::read(font_file).expect("Failed to open font file");

    let font_data = egui::FontData::from_owned(font_file_bytes);
    let mut font_def = egui::FontDefinitions::default();
    font_def.font_data.insert(font_name.to_string(), font_data);

    let font_family: egui::FontFamily = egui::FontFamily::Proportional;
    font_def
        .families
        .get_mut(&font_family)
        .expect("Failed to setup font")
        .insert(0, font_name);

    ctx.set_fonts(font_def);
}

struct TabViewer<'a> {
    world: &'a mut World,
    registry: &'a mut TabRegistry,
}

#[derive(Resource)]
struct UiState {
    state: DockState<EditorTab>,
}

impl UiState {
    fn new() -> Self {
        let mut state = DockState::new(vec![EditorTab::Game]);
        let tree = state.main_surface_mut();
        let [game, timeline] = tree.split_left(
            NodeIndex::root(),
            2.0 / 3.0,
            vec![EditorTab::Timeline, EditorTab::Settings],
        );

        let [_line_list, _timeline] =
            tree.split_left(timeline, 1.0 / 4.0, vec![EditorTab::LineList]);

        let [_, inspector] = tree.split_below(game, 2.0 / 5.0, vec![EditorTab::Inspector]);
        tree.split_right(inspector, 1.0 / 2.0, vec![EditorTab::TimelineSetting]);

        Self { state }
    }

    fn ui(&mut self, world: &mut World, registry: &mut TabRegistry, ctx: &mut egui::Context) {
        let mut tab_viewer = TabViewer { world, registry };

        DockArea::new(&mut self.state)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut tab_viewer);
    }
}

impl egui_dock::TabViewer for TabViewer<'_> {
    type Tab = EditorTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        self.registry
            .get(tab)
            .map(|t| t!(t.title()))
            .unwrap_or("Unknown".into())
            .into()
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        self.registry.tab_ui(ui, self.world, tab);
        match tab {
            EditorTab::Game => {
                let aspect_ratio = &self
                    .world
                    .resource::<Persistent<EditorSettings>>()
                    .game
                    .aspect_ratio;
                let clip_rect = ui.clip_rect();
                let viewport = match aspect_ratio {
                    AspectRatio::Free => clip_rect,
                    AspectRatio::Fixed { width, height } => {
                        utils::misc::keep_aspect_ratio(clip_rect, width / height)
                    }
                };

                let mut game_viewport = self.world.resource_mut::<GameViewport>();
                game_viewport.0 = viewport.into_bevy();

                let mut game_viewport = self.world.resource_mut::<phichain_game::GameViewport>();
                game_viewport.0 = viewport.into_bevy();
            }
            EditorTab::Timeline => {
                let mut timeline_viewport = self.world.resource_mut::<TimelineViewport>();
                let clip_rect = ui.clip_rect();
                timeline_viewport.0 = Rect::from_corners(
                    Vec2 {
                        x: clip_rect.min.x,
                        y: clip_rect.min.y,
                    },
                    Vec2 {
                        x: clip_rect.max.x,
                        y: clip_rect.max.y,
                    },
                );
            }
            _ => {}
        }
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        self.allowed_in_windows(tab)
    }

    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        !matches!(tab, EditorTab::Game)
    }

    fn clear_background(&self, tab: &Self::Tab) -> bool {
        !matches!(tab, EditorTab::Game | EditorTab::Timeline)
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        match tab {
            EditorTab::Game | EditorTab::Timeline => [false, false],
            _ => [true, true],
        }
    }
}

fn ui_system(world: &mut World) {
    let Ok(egui_context) = world.query::<&mut EguiContext>().get_single_mut(world) else {
        return;
    };
    let mut egui_context = egui_context.clone();
    let ctx = egui_context.get_mut();

    let diagnostics = world.resource::<DiagnosticsStore>();
    let mut fps = 0.0;
    if let Some(value) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    {
        fps = value;
    }

    egui::TopBottomPanel::top("phichain.MenuBar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button(t!("menu_bar.file.title"), |ui| {
                if ui.button(t!("menu_bar.file.save")).clicked() {
                    world.resource_scope(|world, mut registry: Mut<ActionRegistry>| {
                        registry.run_action(world, "phichain.project.save");
                    });
                    ui.close_menu();
                }
                if ui.button(t!("menu_bar.file.close")).clicked() {
                    world.resource_scope(|world, mut registry: Mut<ActionRegistry>| {
                        registry.run_action(world, "phichain.project.unload");
                    });
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(t!("menu_bar.file.quit")).clicked() {
                    std::process::exit(0);
                }
            });
            ui.menu_button(t!("menu_bar.tabs.title"), |ui| {
                world.resource_scope(|world, mut ui_state: Mut<UiState>| {
                    world.resource_scope(|_, registry: Mut<TabRegistry>| {
                        for (tab, registered_tab) in registry.iter() {
                            let opened = ui_state
                                .state
                                .iter_all_tabs()
                                .map(|x| x.1)
                                .collect::<Vec<_>>()
                                .contains(&tab);
                            if ui
                                .selectable_label(opened, t!(registered_tab.title()))
                                .clicked()
                            {
                                if opened {
                                    if let Some(node) = ui_state.state.find_tab(tab) {
                                        ui_state.state.remove_tab(node);
                                    }
                                    ui.close_menu();
                                } else {
                                    ui_state.state.add_window(vec![*tab]);
                                    ui.close_menu();
                                }
                            }
                        }
                    });
                });
            });

            ui.menu_button(t!("menu_bar.export.title"), |ui| {
                if ui.button(t!("menu_bar.export.as_official")).clicked() {
                    pick_folder(world, PickingKind::ExportOfficial, FileDialog::new());
                    ui.close_menu();
                }
            });
        });

        ui.add(
            egui::Separator::default()
                .spacing(1.0)
                // fill the left and right gap
                .grow(20.0),
        );

        quick_action(ui, world);

        ui.add_space(1.0);
    });

    let notes: Vec<_> = world.query::<&Note>().iter(world).collect();
    let notes = notes.len();
    let events: Vec<_> = world.query::<&LineEvent>().iter(world).collect();
    let events = events.len();

    let selected_notes: Vec<_> = world
        .query_filtered::<&Note, With<Selected>>()
        .iter(world)
        .collect();
    let selected_notes = selected_notes.len();
    let selected_events: Vec<_> = world
        .query_filtered::<&LineEvent, With<Selected>>()
        .iter(world)
        .collect();
    let selected_events = selected_events.len();

    egui::TopBottomPanel::bottom("phichain.StatusBar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("Phichain v{}", env!("CARGO_PKG_VERSION")));

            ui.label(format!("FPS: {:.2}", fps));

            ui.label(format!("Notes: {}", notes));
            ui.label(format!("Events: {}", events));

            ui.label(format!("Selected Notes: {}", selected_notes));
            ui.label(format!("Selected Events: {}", selected_events));

            world.resource_scope(|_world: &mut World, history: Mut<EditorHistory>| {
                if !history.0.is_saved() {
                    ui.label("*");
                }
            });
        });
    });

    egui::CentralPanel::default()
        .frame(Frame {
            fill: Color32::TRANSPARENT,
            ..default()
        })
        .show(ctx, |_ui| {
            world.resource_scope(|world: &mut World, mut registry: Mut<TabRegistry>| {
                world.resource_scope(|world: &mut World, mut ui_state: Mut<UiState>| {
                    ui_state.ui(world, &mut registry, &mut ctx.clone());
                });
            });
        });
}

fn setup_plugin(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle {
            camera: Camera {
                order: 0,
                ..default()
            },
            ..default()
        },
        GameCamera,
    ));
}
