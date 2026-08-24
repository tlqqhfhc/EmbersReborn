//! Zombie: a melee mob that chases the player and strikes with the weapon it
//! spawned with (sword / spear / bare hands).

use super::ai::{AiPerception, AiTarget, AttackCooldown, HitStun, LootTable, chase};
use super::attributes::MovementSpeed;
use super::living_actor;
use super::{Attributes, Damage, DamageKnockback, DamageSource};
use crate::dim::Movements;
use crate::dim::item::MeleeStrike;
use crate::dim::item::embers::EMBER_SHARD;
use crate::utils::NamespacedKey;
use crate::utils::SystemRng;
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_tnua::prelude::*;
use rand::RngExt;
use rand::rngs::SmallRng;
use std::ops::RangeInclusive;
use std::sync::LazyLock;

pub static KEY: LazyLock<NamespacedKey> = LazyLock::new(|| NamespacedKey::new_embers("zombie"));

/// Per-zombie finite-state machine.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum ZombieState {
    /// Resting state when the target is lost.
    #[default]
    Idle,
    /// Walking toward the player.
    Chase,
    /// Just struck; back to chasing on the next frame.
    Attack,
}

/// Weapon a zombie spawns with.
#[derive(Clone, Copy, Component, Debug, Default, PartialEq)]
pub enum ZombieWeapon {
    #[default]
    BareHand,
    Sword,
    Spear,
}

impl ZombieWeapon {
    /// Rolls the spawn weapon with a 4/3/3 sword/spear/bare-hand ratio.
    pub fn roll(rng: &mut impl RngExt) -> Self {
        match rng.random_range(0..10) {
            0..=3 => Self::Sword,
            4..=6 => Self::Spear,
            _ => Self::BareHand,
        }
    }
}

/// Strike stats for weapon-wielding zombies; mirror the player weapon tomls
/// (`pld/global/item_actions/embers/`) at mob-appropriate ranges.
static SWORD_STRIKE: LazyLock<MeleeStrike> =
    LazyLock::new(|| MeleeStrike::new(6., 6., 120., 2.).expect("valid zombie sword strike"));
static SPEAR_STRIKE: LazyLock<MeleeStrike> =
    LazyLock::new(|| MeleeStrike::new(7., 7., 30., 2.5).expect("valid zombie spear strike"));

/// Bare-hand strikes deal a random amount of damage within this range.
const BARE_HAND_DAMAGE: RangeInclusive<f32> = 2.0..=4.0;
const BARE_HAND_KNOCKBACK: f32 = 5.;
const BARE_HAND_RANGE: f32 = 1.5;

#[derive(Clone, Component, Debug)]
#[require(AiTarget, AiPerception, AttackCooldown)]
pub struct Zombie(ZombieState);

impl Default for Zombie {
    fn default() -> Self {
        Self(ZombieState::Idle)
    }
}

pub fn zombie(weapon: ZombieWeapon) -> impl Scene {
    bsn! {
        living_actor(&KEY, false)
        Collider::cylinder(0.5, 1.7)
        Mesh3d(
            asset_value(
                Cylinder {
                    radius: 0.5,
                    half_height: 0.85,
                }
                .mesh(),
            ),
        )
        MeshMaterial3d<StandardMaterial>(asset_value(Color::srgb(0.15, 0.8, 0.1)))
        Zombie(ZombieState::Idle)
        template_value(weapon)
        AiPerception {
            sight_range: 24.,
            attack_range: 2.,
        }
        LootTable({vec![EMBER_SHARD.clone(), EMBER_SHARD.clone()]})
    }
}

/// Applies the zombie's strike: weapon strikes reuse the shared melee fan
/// damage, bare hands deal direct radial damage to the target.
fn strike(
    commands: &mut Commands,
    spatial_query: &mut SpatialQuery,
    rng: &mut SystemRng<SmallRng>,
    zombie: Entity,
    transform: &GlobalTransform,
    weapon: ZombieWeapon,
    target: Entity,
    target_position: Vec3,
) {
    match weapon {
        ZombieWeapon::Sword => SWORD_STRIKE.apply(commands, spatial_query, zombie, transform),
        ZombieWeapon::Spear => SPEAR_STRIKE.apply(commands, spatial_query, zombie, transform),
        ZombieWeapon::BareHand => {
            if transform.translation().distance(target_position) <= BARE_HAND_RANGE {
                commands.write_message(Damage {
                    target,
                    amount: rng.random_range(BARE_HAND_DAMAGE),
                    knockback: DamageKnockback::Radial(BARE_HAND_KNOCKBACK),
                    source: DamageSource {
                        origin: transform.translation(),
                        causing_entity: Some(zombie),
                        direct_entity: Some(zombie),
                    },
                });
            }
        }
    }
}

pub(super) fn system(
    mut commands: Commands,
    mut spatial_query: SpatialQuery,
    mut zombies: Query<
        (
            Entity,
            &GlobalTransform,
            &AiPerception,
            &AiTarget,
            &mut Zombie,
            &mut TnuaController<Movements>,
            &mut AttackCooldown,
            &Attributes<MovementSpeed>,
            &ZombieWeapon,
            Option<&HitStun>,
        ),
        Without<super::player::Player>,
    >,
    targets: Query<&GlobalTransform, With<super::player::Player>>,
    mut rng: ResMut<SystemRng<SmallRng>>,
) {
    for (
        entity,
        transform,
        perception,
        target,
        mut zombie,
        mut controller,
        mut cooldown,
        movement_speed,
        weapon,
        hit_stun,
    ) in zombies.iter_mut()
    {
        let target_translation = target.0.and_then(|target| targets.get(target).ok());
        let my_translation = transform.translation();
        let target_pos = target_translation.map(|transform| transform.translation());

        // While stunned, skip state advancement (still let the physics knockback settle).
        let stunned = hit_stun.is_some_and(|hit_stun| !hit_stun.0.is_finished());

        // Advance the state machine.
        let mut next_state = zombie.0;
        if !stunned {
            match zombie.0 {
                ZombieState::Idle => {
                    if target.0.is_some() {
                        next_state = ZombieState::Chase;
                    }
                }
                ZombieState::Chase => {
                    if target.0.is_none() {
                        next_state = ZombieState::Idle;
                    }
                }
                ZombieState::Attack => {
                    next_state = ZombieState::Chase;
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

        // Strike when chasing, in range and off cooldown.
        if next_state == ZombieState::Chase
            && in_range
            && !stunned
            && cooldown.0.is_finished()
            && let Some(target_entity) = target.0
        {
            strike(
                &mut commands,
                &mut spatial_query,
                &mut rng,
                entity,
                transform,
                *weapon,
                target_entity,
                target_pos.unwrap(),
            );
            cooldown.0.reset();
            next_state = ZombieState::Attack;
        }

        zombie.0 = next_state;
    }
}
