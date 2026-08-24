//! Floating damage numbers rendered as UI text projected from world space.

use super::DamageNumber;
use crate::pld::foundry::text_font;
use crate::ui::dim::{DimensionViewNode, PlayerCamera};
use crate::utils::NamespacedKey;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::sync::LazyLock;

static FONT_KEY: LazyLock<NamespacedKey> = LazyLock::new(|| NamespacedKey::new_embers("polygon"));

/// A floating damage number: its world-space origin plus a lifetime counter.
#[derive(Component, Clone, Default)]
struct FloatingText {
    world_position: Vec3,
    timer: Timer,
    drift: f32,
}

const DURATION_SECS: f32 = 0.8;
/// Upward drift in world units over the whole lifetime.
const DRIFT_WORLD: f32 = 0.8;

fn spawn_damage_numbers(
    mut reader: MessageReader<DamageNumber>,
    mut commands: Commands,
    dimension_view_node: Single<Entity, With<DimensionViewNode>>,
) {
    for DamageNumber { position, amount } in reader.read() {
        let (position, amount) = (*position, *amount);
        commands.spawn_scene(bsn! {
            ChildOf({*dimension_view_node})
            Text::new(format!("{:.0}", amount))
            text_font(&*FONT_KEY, 14.)
            TextColor(Color::srgb(1.0, 0.35, 0.35))
            TextLayout
            Node {
                position_type: PositionType::Absolute,
            }
            FloatingText {
                world_position: {position},
                timer: {Timer::from_seconds(DURATION_SECS, TimerMode::Once)},
                drift: 0.,
            }
        });
    }
}

fn update_damage_numbers(
    mut commands: Commands,
    camera: Single<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut floats: Query<(Entity, &mut FloatingText, &mut Node, &mut TextColor)>,
    time: Res<Time>,
) {
    let (camera, camera_transform) = *camera;
    for (entity, mut floating, mut node, mut text_color) in floats.iter_mut() {
        floating.timer.tick(time.delta());
        floating.drift += (DRIFT_WORLD / DURATION_SECS) * time.delta_secs();

        if floating.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }

        // Project the drifting world position to the window and place the UI node.
        let world_pos = floating.world_position + Vec3::Y * floating.drift;
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            let scale = window.scale_factor();
            node.left = Val::Px(viewport_pos.x / scale);
            node.top = Val::Px(viewport_pos.y / scale);
        }

        // Fade out over the lifetime.
        let remaining = floating.timer.remaining_secs();
        let alpha = (remaining / DURATION_SECS).clamp(0., 1.);
        text_color.0.set_alpha(alpha);
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, spawn_damage_numbers)
        .add_systems(Update, update_damage_numbers);
}
