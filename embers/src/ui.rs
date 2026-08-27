pub mod crafting;
pub mod death_screen;
pub mod dim;
pub mod gateway_menu;
pub mod heads_up_display;
pub mod inventory;
pub mod loading_screen;
pub mod main_menu;
pub mod options_main;
pub mod options_video;
pub mod pause_screen;
pub mod title_screen;

use crate::pld::Payload;
use crate::pld::foundry::{text_font, ui_image_node};
use crate::utils::NamespacedKey;
use bevy::asset::AssetPath;
use bevy::color::palettes::css::WHITE;
use bevy::ecs::system::{IntoObserverSystem, NonSendMarker, ObserverSystem};
use bevy::input_focus::{FocusGained, FocusLost, InputFocus};
use bevy::math::CompassOctant;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::scene::EntityWorldMutSceneExt;
use bevy::ui::InteractionDisabled;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use serde::Deserialize;
use std::sync::LazyLock;
use winit::window::Icon;

#[derive(Clone, Component, Default)]
pub struct RootNode;

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum GameState {
    Dimension,
    #[default]
    MainMenu,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum ActiveOverlay {
    Crafting,
    DeathScreen,
    GatewayMenu,
    HeadsUpDisplay,
    Inventory,
    #[default]
    LoadingScreen,
    OptionsAudio,
    OptionsControls,
    OptionsLanguage,
    OptionsMain,
    OptionsVideo,
    PauseScreen,
    TitleScreen,
}

fn process_escaping(
    keys: Res<ButtonInput<KeyCode>>,
    game_state: Res<State<GameState>>,
    active_overlay: Res<State<ActiveOverlay>>,
    mut next_overlay: ResMut<NextState<ActiveOverlay>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        match **active_overlay {
            ActiveOverlay::HeadsUpDisplay => next_overlay.set(ActiveOverlay::PauseScreen),
            ActiveOverlay::Crafting
            | ActiveOverlay::GatewayMenu
            | ActiveOverlay::Inventory
            | ActiveOverlay::PauseScreen => next_overlay.set(ActiveOverlay::HeadsUpDisplay),
            ActiveOverlay::OptionsAudio
            | ActiveOverlay::OptionsControls
            | ActiveOverlay::OptionsLanguage
            | ActiveOverlay::OptionsVideo => next_overlay.set(ActiveOverlay::OptionsMain),
            ActiveOverlay::OptionsMain => match **game_state {
                GameState::Dimension => next_overlay.set(ActiveOverlay::HeadsUpDisplay),
                GameState::MainMenu => next_overlay.set(ActiveOverlay::TitleScreen),
            },
            // Loading and Title screens do not react to Escape.
            ActiveOverlay::LoadingScreen | ActiveOverlay::TitleScreen => {}
            // The death screen is only left through the Respawn button.
            ActiveOverlay::DeathScreen => {}
        }
    }
}

fn process_directional_navigation(
    keys: Res<ButtonInput<KeyCode>>,
    mut navigator: AutoDirectionalNavigator,
) {
    let Some(direction) = Dir2::from_xy(
        (keys.just_pressed(KeyCode::ArrowRight) as i8 - keys.just_pressed(KeyCode::ArrowLeft) as i8)
            as f32,
        (keys.just_pressed(KeyCode::ArrowUp) as i8 - keys.just_pressed(KeyCode::ArrowDown) as i8)
            as f32,
    )
    .ok()
    .map(CompassOctant::from) else {
        return;
    };
    let _result = navigator.navigate(direction);
}

#[derive(Clone, Debug, EntityEvent, PartialEq)]
pub struct NodeInteraction<Ext: Send + Sync + 'static = ()> {
    pub entity: Entity,
    pub extra: Ext,
}

impl<Ext: Send + Sync + 'static> NodeInteraction<Ext> {
    pub fn new(entity: Entity, extra: Ext) -> Self {
        Self { entity, extra }
    }
}

fn trigger_default_node_interaction(
    mut commands: Commands,
    focus: Res<InputFocus>,
    keys: Res<ButtonInput<KeyCode>>,
    interactions: Query<
        (Entity, &Interaction),
        (
            Changed<Interaction>,
            Without<InteractionDisabled>,
            With<Button>,
        ),
    >,
) {
    if keys.just_pressed(KeyCode::Enter)
        && let Some(node) = focus.get()
    {
        commands.trigger(NodeInteraction::new(node, ()));
        //commands.spawn((AudioPlayer::new(), PlaybackSettings::DESPAWN));
    }
    for (node, interaction) in interactions.iter() {
        if matches!(interaction, Interaction::Pressed) {
            commands.trigger(NodeInteraction::new(node, ()));
        }
    }
}

static UI_FONT: LazyLock<NamespacedKey> = LazyLock::new(|| NamespacedKey::new_embers("polygon"));

fn text(text: impl Into<String>, color: impl Into<Color>, size: impl Into<FontSize>) -> impl Scene {
    bsn! {
        Text(text)
        TextColor(color)
        text_font(&*UI_FONT, size)
        TextLayout
    }
}

fn text_button<M: 'static>(
    label: impl Into<String>,
    action: impl IntoObserverSystem<NodeInteraction, (), M> + Clone + Sync,
) -> impl Scene {
    fn update_image_node<E: EntityEvent, B: Bundle>() -> impl ObserverSystem<E, B> + Clone {
        IntoSystem::into_system(
            |event: On<E, B>,
             mut commands: Commands,
             focus: Res<InputFocus>,
             status: Query<(&Hovered, Has<InteractionDisabled>), With<Button>>| {
                let entity = event.event_target();
                // Also fires while the button is being despawned (DespawnOnExit on
                // overlay switch triggers Remove observers), so bail out early if
                // the button is already gone.
                let Some((Hovered(hovered), disabled)) = status.get(entity).ok() else {
                    return;
                };
                let highlighted = *hovered
                    || focus
                        .get()
                        .is_some_and(|focus_entity| focus_entity == entity);
                // The entity may die before this command is applied, so apply the
                // image defensively instead of panicking on a stale entity.
                commands.queue(move |world: &mut World| {
                    if let Some(mut entity) = world.get_entity_mut(entity).ok() {
                        entity.apply_scene(ui_image_node(match (highlighted, disabled) {
                            (false, false) => "widgets/button",
                            (true, false) => "widgets/button_highlighted",
                            (_highlighted, true) => "widgets/button_disabled",
                        }));
                    }
                });
            },
        )
    }
    bsn! {
        Button
        Node {
            width: px(200),
            height: px(20),
            margin: px(2),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        ui_image_node("widgets/button")
        Hovered
        on(update_image_node::<Insert, Hovered>())
        on(update_image_node::<FocusGained, ()>())
        on(update_image_node::<FocusLost, ()>())
        on(update_image_node::<Add, InteractionDisabled>())
        on(update_image_node::<Remove, InteractionDisabled>())
        on(action)
        Children [
            (text(label, WHITE, 14.)),
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
enum SliceScaling {
    Stretch,
    Tile { stretch_value: f32 },
}

impl From<SliceScaling> for SliceScaleMode {
    fn from(value: SliceScaling) -> Self {
        match value {
            SliceScaling::Stretch => Self::Stretch,
            SliceScaling::Tile { stretch_value } => Self::Tile { stretch_value },
        }
    }
}

#[derive(Asset, Clone, Debug, Default, Deserialize, TypePath)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TextureScaling {
    Auto,
    #[default]
    Stretch,
    Sliced {
        border_width_min: f32,
        border_width_max: f32,
        border_height_min: f32,
        border_height_max: f32,
        center_scaling: SliceScaling,
        side_scaling: SliceScaling,
        max_corner_scale: f32,
    },
    Tiled {
        tile_x: bool,
        tile_y: bool,
        stretch_value: f32,
    },
}

impl From<&TextureScaling> for NodeImageMode {
    fn from(value: &TextureScaling) -> Self {
        match value {
            TextureScaling::Auto => Self::Auto,
            TextureScaling::Stretch => Self::Stretch,
            &TextureScaling::Sliced {
                border_width_min,
                border_width_max,
                border_height_min,
                border_height_max,
                center_scaling,
                side_scaling,
                max_corner_scale,
            } => Self::Sliced(TextureSlicer {
                border: BorderRect {
                    min_inset: Vec2::new(border_width_min, border_height_min),
                    max_inset: Vec2::new(border_width_max, border_height_max),
                },
                center_scale_mode: center_scaling.into(),
                sides_scale_mode: side_scaling.into(),
                max_corner_scale,
            }),
            &TextureScaling::Tiled {
                tile_x,
                tile_y,
                stretch_value,
            } => Self::Tiled {
                tile_x,
                tile_y,
                stretch_value,
            },
        }
    }
}

impl Payload for TextureScaling {
    fn payload_root() -> AssetPath<'static> {
        "textures".into()
    }
}

#[derive(Asset, Debug, Deserialize, TypePath, Clone, PartialEq)]
pub struct TextureAnimation {
    atlas_begin_index: usize,
    atlas_end_index: usize,
    frame_time_secs: f32,
}

impl Payload for TextureAnimation {
    fn payload_root() -> AssetPath<'static> {
        "textures".into()
    }
}

#[derive(Clone, Component, Debug, PartialEq)]
pub struct AnimatedTexture {
    animation: TextureAnimation,
    timer: Timer,
}

impl AnimatedTexture {
    pub fn new(animation: TextureAnimation) -> Self {
        Self {
            timer: Timer::from_seconds(animation.frame_time_secs, TimerMode::Repeating),
            animation,
        }
    }
}

fn run_animations(time: Res<Time>, mut animated: Query<(&mut ImageNode, &mut AnimatedTexture)>) {
    for (mut image_node, mut animated_texture) in animated.iter_mut() {
        let &mut AnimatedTexture {
            ref animation,
            ref mut timer,
        } = &mut *animated_texture;
        let Some(atlas) = &mut image_node.texture_atlas else {
            continue;
        };
        timer.tick(time.delta());
        if timer.just_finished() {
            atlas.index = atlas.index.wrapping_add(1);
            if atlas.index >= animation.atlas_end_index || atlas.index < animation.atlas_begin_index
            {
                atlas.index = animation.atlas_begin_index;
            }
        }
    }
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct SetWindowIcon {
    pub image: Handle<Image>,
}

fn set_window_icons(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    mut windows: Query<(Entity, &mut Window, &SetWindowIcon)>,
    _non_send_marker: NonSendMarker,
) {
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        for (window_entity, mut window, set_window_icon) in windows.iter_mut() {
            window.visible = true;
            let Some(winit_window) = winit_windows.get_window(window_entity) else {
                continue;
            };
            let Some(window_icon) = images.get(&set_window_icon.image) else {
                continue;
            };
            winit_window.set_window_icon(
                Icon::from_rgba(
                    window_icon.data.clone().unwrap(),
                    window_icon.width(),
                    window_icon.height(),
                )
                .ok(),
            );
            commands.entity(window_entity).remove::<SetWindowIcon>();
        }
    })
}

pub(super) fn plugin(app: &mut App) {
    app.init_state::<GameState>()
        .init_state::<ActiveOverlay>()
        .init_asset::<TextureAnimation>()
        .init_asset::<TextureScaling>()
        .insert_resource(UiScale(3.))
        .add_systems(PreUpdate, process_escaping)
        .add_systems(PreUpdate, process_directional_navigation)
        .add_systems(Update, trigger_default_node_interaction)
        .add_systems(Update, run_animations)
        .add_systems(Update, set_window_icons)
        .add_systems(
            PreStartup,
            |mut commands: Commands,
             asset_server: Res<AssetServer>,
             primary_window: Single<Entity, With<PrimaryWindow>>| {
                commands.entity(*primary_window).insert(SetWindowIcon {
                    image: asset_server.load("embedded://embers/icon.png"),
                });
            },
        )
        .add_plugins((
            crafting::plugin,
            death_screen::plugin,
            dim::plugin,
            heads_up_display::plugin,
            inventory::plugin,
            gateway_menu::plugin,
            loading_screen::plugin,
            main_menu::plugin,
            options_main::plugin,
            pause_screen::plugin,
            title_screen::plugin,
        ));
}
