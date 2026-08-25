use super::ai::{AiPerception, AiTarget, HitStun, chase};
use super::attributes::MovementSpeed;
use super::living_actor;
use super::{Attributes, DamageSource};
use crate::dim::{Explosion, Movements};
use crate::utils::NamespacedKey;
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_tnua::prelude::*;
use std::sync::LazyLock;

pub static KEY: LazyLock<NamespacedKey> = LazyLock::new(|| NamespacedKey::new_embers("creeper"));

/// Per-creeper finite-state machine.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum CreeperState {
    /// Resting state when the target is lost.
    #[default]
    Idle,
    /// Walking toward the player.
    Chase,
    /// Counting down in place before exploding. `remaining` is seconds left.
    Fuse(f32),
}

/// Fuse countdown duration in seconds.
const FUSE_SECS: f32 = 1.5;
/// Explosion power passed through to [`Explosion`].
const FUSE_POWER: f32 = 4.;

#[derive(Clone, Component, Debug)]
#[require(AiTarget, AiPerception)]
pub struct Creeper(CreeperState);

impl Default for Creeper {
    fn default() -> Self {
        Self(CreeperState::Idle)
    }
}

pub fn creeper() -> impl Scene {
    bsn! {
        living_actor(&KEY, false)
        Collider::cylinder(0.5, 1.7)
        Mesh3d(asset_value(
            Cylinder { radius: 0.5, half_height: 0.85 }.mesh(),
        ),)
        MeshMaterial3d::<StandardMaterial>(asset_value(Color::srgb(0.1, 0.35, 0.1)))
        Creeper(CreeperState::Idle)
        AiPerception {
            sight_range: 20.,
            attack_range: 1.5,
        }
    }
}

pub(super) fn system(
    mut commands: Commands,
    mut creepers: Query<
        (
            Entity,
            &GlobalTransform,
            &AiPerception,
            &AiTarget,
            &mut Creeper,
            &mut TnuaController<Movements>,
            &Attributes<MovementSpeed>,
            Option<&HitStun>,
        ),
        Without<super::player::Player>,
    >,
    targets: Query<&GlobalTransform, With<super::player::Player>>,
    time: Res<Time>,
) {
    for (
        entity,
        transform,
        perception,
        target,
        mut creeper,
        mut controller,
        movement_speed,
        hit_stun,
    ) in creepers.iter_mut()
    {
        let target_translation = target.0.and_then(|t| targets.get(t).ok());
        let my_translation = transform.translation();
        let target_pos = target_translation.map(|t| t.translation());

        // While stunned, skip state advancement (still let the physics knockback settle).
        let stunned = hit_stun.is_some_and(|hs| !hs.0.is_finished());

        // Advance the state machine.
        let mut next_state = creeper.0;
        if !stunned {
            match creeper.0 {
                CreeperState::Idle => {
                    if target.0.is_some() {
                        next_state = CreeperState::Chase;
                    }
                }
                CreeperState::Chase => {
                    if target.0.is_none() {
                        next_state = CreeperState::Idle;
                    }
                }
                CreeperState::Fuse(remaining) => {
                    let remaining = remaining - time.delta_secs();
                    if remaining <= 0. {
                        commands.trigger(Explosion {
                            power: FUSE_POWER,
                            source: DamageSource {
                                origin: my_translation,
                                causing_entity: Some(entity),
                                direct_entity: Some(entity),
                            },
                        });
                        commands.entity(entity).despawn();
                        continue;
                    }
                    next_state = CreeperState::Fuse(remaining);
                }
            }
        }

        // Movement + proximity check.
        let in_range = chase(
            &mut controller,
            perception,
            my_translation,
            target_pos,
            movement_speed.value(),
        );

        // Only transition into Fuse while chasing and in range.
        if next_state == CreeperState::Chase && in_range {
            next_state = CreeperState::Fuse(FUSE_SECS);
        }

        creeper.0 = next_state;
    }
}
