//! Shared building blocks for mob (non-player living actor) AI.
//!
//! The AI model is a per-mob state machine (`enum` + one dedicated system per mob).
//! This module provides reusable components and helpers: target perception, chase
//! movement through the shared [`Movements`] Tnua scheme, attack cooldown, hit-stun
//! and a death loot table.

use super::Dead;
use super::player::Player;
use crate::dim::Movements;
use crate::utils::NamespacedKey;
use bevy::prelude::*;
use bevy_tnua::builtins::TnuaBuiltinWalk;
use bevy_tnua::prelude::*;

/// The entity (currently a [`Player`]) that this mob is chasing.
#[derive(Component, Debug, Default)]
pub struct AiTarget(pub Option<Entity>);

/// Static perception ranges for a mob.
#[derive(Component, Clone, Debug)]
pub struct AiPerception {
    /// Distance within which the mob notices its target.
    pub sight_range: f32,
    /// Distance within which the mob may act (attack / explode / ...).
    pub attack_range: f32,
}

impl Default for AiPerception {
    fn default() -> Self {
        Self {
            sight_range: 24.,
            attack_range: 2.,
        }
    }
}

/// Attack cooldown used by mobs that strike periodically.
#[derive(Component, Debug)]
pub struct AttackCooldown(pub Timer);

impl Default for AttackCooldown {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0, TimerMode::Repeating))
    }
}

/// Post-hit stun: a mob that still has an active [`HitStun`] skips its own attack
/// decisions, on top of the positional knockback already applied via Tnua.
#[derive(Component, Debug)]
pub struct HitStun(pub Timer);

impl Default for HitStun {
    fn default() -> Self {
        Self(Timer::from_seconds(0.25, TimerMode::Once))
    }
}

/// Loot dropped when this living actor dies. Each entry is a [`NamespacedKey`] of an
/// item to spawn as an [`crate::dim::actor::item_actor::ItemActor`]; repeated entries
/// drop multiple items.
#[derive(Component, Clone, Debug, Default)]
pub struct LootTable(pub Vec<NamespacedKey>);

/// Every frame, store the closest [`Player`] within each mob's [`AiPerception::sight_range`]
/// into [`AiTarget`]; out-of-range targets are cleared.
pub(super) fn perceive_targets(
    mut mobs: Query<(Entity, &GlobalTransform, &AiPerception, &mut AiTarget), Without<Player>>,
    players: Query<(Entity, &GlobalTransform), (With<Player>, Without<Dead>)>,
) {
    for (_mob, mob_transform, perception, mut target) in mobs.iter_mut() {
        target.0 = players
            .iter()
            .map(|(entity, player_transform)| {
                (
                    entity,
                    player_transform
                        .translation()
                        .distance(mob_transform.translation()),
                )
            })
            .filter(|(_, distance)| *distance <= perception.sight_range)
            .min_by(|(_, lhs), (_, rhs)| f32::total_cmp(lhs, rhs))
            .map(|(entity, _)| entity);
    }
}

/// Drive mob movement toward `target_translation` through the shared [`Movements`] Tnua scheme.
///
/// `speed` scales the desired motion, mirroring how the player scales its movement by the
/// `movement_speed` attribute (the Tnua walk config speed is otherwise a constant).
///
/// Returns `true` when `target_translation` is active and within `perception.attack_range`.
pub(super) fn chase(
    controller: &mut TnuaController<Movements>,
    perception: &AiPerception,
    my_translation: Vec3,
    target_translation: Option<Vec3>,
    speed: f32,
) -> bool {
    let Some(target_translation) = target_translation else {
        controller.basis = TnuaBuiltinWalk {
            desired_motion: Vec3::ZERO,
            ..default()
        };
        controller.initiate_action_feeding();
        return false;
    };
    let to_target = target_translation - my_translation;
    let distance = to_target.length();
    let direction = to_target.try_normalize().unwrap_or(Vec3::ZERO);
    controller.basis = TnuaBuiltinWalk {
        desired_motion: direction * speed,
        desired_forward: Dir3::new(direction).ok(),
    };
    controller.initiate_action_feeding();
    distance <= perception.attack_range
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (perceive_targets, tick_status).run_if(in_state(crate::ui::GameState::Dimension)),
    );
}

/// Advances any active [`HitStun`] / [`AttackCooldown`] timers.
fn tick_status(
    mut hit_stuns: Query<&mut HitStun>,
    mut cooldowns: Query<&mut AttackCooldown>,
    time: Res<Time>,
) {
    for mut hit_stun in hit_stuns.iter_mut() {
        hit_stun.0.tick(time.delta());
    }
    for mut cooldown in cooldowns.iter_mut() {
        cooldown.0.tick(time.delta());
    }
}
