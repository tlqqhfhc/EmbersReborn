#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod balance;
pub mod dim;
pub mod input;
pub mod pld;
pub mod ui;
pub mod utils;

use avian3d::prelude::*;
use bevy::asset::UnapprovedPathMode;
use bevy::image::ImageSamplerDescriptor;
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::settings::SettingsGroup;
use bevy::window::WindowTheme;
use bevy_sprinkles::prelude::*;
use bevy_tnua::prelude::*;
use bevy_tnua_avian3d::prelude::*;
use dim::{Movements, SourceExclusionCollisionHooks};
use std::env::current_exe;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use ui::loading_screen::{Load, MainMenuEntryContext};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Reflect, Resource, SettingsGroup)]
struct Options {}

pub static UNPROCESSED_ASSETS_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub static ASSETS_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn main() {
    let mut app = App::new();
    app.add_plugins(LogPlugin { ..default() });
    let mut current_path = current_exe().unwrap();
    while let Ok(destination) = current_path.read_link() {
        current_path = destination;
    }
    let find_resource_root = |folder, marker| {
        let mut path = current_path.clone();
        while path.pop() {
            let resources = path.join(folder);
            if resources.is_dir() && resources.join(marker).exists() {
                return Some(resources);
            }
        }
        None
    };
    #[cfg(debug_assertions)]
    UNPROCESSED_ASSETS_ROOT
        .set(
            find_resource_root("pld", ".embers_payload_root")
                .inspect(|path| info!("Found payload root: {}", path.display()))
                .unwrap_or_else(|| {
                    warn!("Could not find payload root!");
                    Path::new("pld").to_path_buf()
                }),
        )
        .unwrap();
    ASSETS_ROOT
        .set(
            find_resource_root("shp", ".embers_shipment_root")
                .inspect(|path| info!("Found shipment root: {}", path.display()))
                .unwrap_or_else(|| {
                    error!("Could not find shipment root!");
                    Path::new("shp").to_path_buf()
                }),
        )
        .unwrap();
    app.add_plugins(
        DefaultPlugins
            .build()
            .set(AssetPlugin {
                file_path: UNPROCESSED_ASSETS_ROOT
                    .get()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                processed_file_path: ASSETS_ROOT.get().unwrap().to_string_lossy().to_string(),
                mode: AssetMode::Processed,
                unapproved_path_mode: UnapprovedPathMode::Deny,
                ..default()
            })
            .set(ImagePlugin {
                default_sampler: ImageSamplerDescriptor::nearest(),
                ..default()
            })
            .disable::<LogPlugin>()
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Embers".to_string(),
                    window_theme: Some(WindowTheme::Dark),
                    visible: false,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(DirectionalNavigationPlugin)
    .add_plugins(PhysicsPlugins::default().with_collision_hooks::<SourceExclusionCollisionHooks>())
    .add_plugins(SprinklesPlugin)
    .add_plugins(TabNavigationPlugin)
    .add_plugins(TnuaControllerPlugin::<Movements>::new(PhysicsSchedule))
    .add_plugins(TnuaAvian3dPlugin::new(PhysicsSchedule))
    .add_plugins((dim::plugin, input::plugin, pld::plugin, ui::plugin))
    .add_systems(Startup, |mut commands: Commands| {
        commands.trigger(Load::EnterMainMenu(MainMenuEntryContext::Init))
    })
    .run();
}
