pub mod ai;
pub mod attributes;
pub mod creeper;
pub mod damage_number;
pub mod dummy;
pub mod player;
pub mod zombie;

use super::actor;
use crate::dim::actor::item_actor::item_actor_of;
use crate::dim::item::item_stack;
use crate::dim::{ActiveDimension, Dimension, Movements, MovementsConfig, PhysicsPreset};
use crate::pld::foundry::PayloadTemplate;
use crate::ui::ActiveOverlay;
use crate::utils::{Keyed, NamespacedKey, SystemRng, template_bundle};
use ai::{HitStun, LootTable};
use attributes::{Attributes, AttributesTemplate, DamageTaken, KnockbackTaken, MaxHealth};
use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;
use bevy_tnua::builtins::TnuaBuiltinKnockback;
use bevy_tnua::prelude::*;
use player::{Player, PlayerInventory, SpawnPoint};
use rand::rngs::SmallRng;

#[derive(Component, Debug, Default)]
pub struct LivingActor;

#[derive(Default)]
struct MovementConfigTemplate {
    config: PayloadTemplate<MovementsConfig>,
}

impl Template for MovementConfigTemplate {
    type Output = TnuaConfig<Movements>;
    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        Ok(TnuaConfig(self.config.build_template(context)?))
    }
    fn clone_template(&self) -> Self {
        Self {
            config: self.config.clone_template(),
        }
    }
}

impl MovementConfigTemplate {
    fn new(actor_key: &NamespacedKey) -> Self {
        Self {
            config: PayloadTemplate::path(actor_key),
        }
    }
}

#[derive(Component, Debug)]
#[require(LivingActor)]
pub struct Health(pub f32);

/// Marker for a player that has died and is waiting on the death screen.
/// While present, the player is immune to damage and excluded from mob
/// perception; healing and the warp back to the lobby only happen when the
/// player presses Respawn, so late damage messages cannot chip into a
/// freshly restored health pool.
#[derive(Component, Debug, Default)]
pub struct Dead;

impl FromTemplate for Health {
    type Template = HealthTemplate;
}

#[derive(Default)]
/// # Note
/// HealthTemplate depends on [`AttributesTemplate`], which is a bundle template, so HealthTemplate
/// has to be used as a bundle template as well, as bundle templates are applied after component
/// templates.
pub struct HealthTemplate;

impl Template for HealthTemplate {
    type Output = Health;
    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        Ok(Health(
            context
                .entity
                .get::<Attributes<MaxHealth>>()
                .unwrap()
                .value(),
        ))
    }
    fn clone_template(&self) -> Self {
        Self
    }
}

pub fn living_actor(key: &NamespacedKey, interactable: bool) -> impl Scene {
    bsn! {
        actor()
        { PhysicsPreset::LivingActor.physics(interactable) }
        template_bundle(AttributesTemplate::new(key.clone()))
        template_bundle(HealthTemplate)
        template(|_| Ok(TnuaController::<Movements>::default()))
        template_value(MovementConfigTemplate::new(key))
    }
}

#[derive(Message)]
pub struct Damage {
    pub target: Entity,
    pub amount: f32,
    pub knockback: DamageKnockback,
    pub source: DamageSource,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DamageKnockback {
    Directional(Vec3),
    Radial(f32),
    None,
}

impl Default for DamageKnockback {
    fn default() -> Self {
        DamageKnockback::Radial(20.)
    }
}

#[derive(Clone, Copy, Default)]
pub struct DamageSource {
    pub origin: Vec3,
    pub causing_entity: Option<Entity>,
    pub direct_entity: Option<Entity>,
}

/// Request to render a floating damage number at a world position.
#[derive(Message)]
pub struct DamageNumber {
    pub position: Vec3,
    pub amount: f32,
}

fn damage(
    mut damages: MessageReader<Damage>,
    mut commands: Commands,
    mut living_actors: Query<(
        Entity,
        &GlobalTransform,
        &mut Health,
        &mut Transform,
        &Attributes<DamageTaken>,
        &mut TnuaController<Movements>,
        &Attributes<KnockbackTaken>,
        &Attributes<MaxHealth>,
        Option<&LootTable>,
        Option<&SpawnPoint>,
        Option<&mut PlayerInventory>,
        Has<Player>,
        Option<&Dead>,
    )>,
    active_dimension: Option<Single<&Dimension, With<ActiveDimension>>>,
    active_overlay: Res<State<ActiveOverlay>>,
    mut next_overlay: ResMut<NextState<ActiveOverlay>>,
) {
    // Entities already killed earlier in this pass. Their despawn is still a
    // pending command, so `get_mut` would still succeed for further messages
    // targeting them; without this guard the death branch would run twice,
    // queueing a second despawn (command error panic) and dropping loot twice.
    let mut dead = Vec::new();
    for Damage {
        target,
        amount,
        knockback,
        source:
            DamageSource {
                origin,
                causing_entity: _,
                direct_entity: _,
            },
    } in damages.read()
    {
        let Ok((
            entity,
            transform,
            mut health,
            mut local_transform,
            damage_taken,
            mut controller,
            knockback_taken,
            max_health,
            loot_table,
            spawn_point,
            inventory,
            is_player,
            is_dead,
        )) = living_actors.get_mut(*target)
        else {
            warn!("Could not damage nonexistent living actor {}", target);
            continue;
        };
        // A dead player (waiting on the death screen) is immune to damage.
        if dead.contains(&entity) || is_dead.is_some() {
            continue;
        }
        let dealt = damage_taken.value_for(*amount).max(0.);
        health.0 -= dealt;

        // Floating damage number.
        commands.write_message(DamageNumber {
            position: transform.translation() + Vec3::Y * 1.5,
            amount: dealt,
        });

        // Knockback.
        if let Some(knockback) = match *knockback {
            DamageKnockback::Directional(vector) => Some(vector),
            DamageKnockback::Radial(scalar) => {
                Some(scalar * (transform.translation() - origin).normalize_or_zero())
            }
            DamageKnockback::None => None,
        } {
            controller.action_interrupt(Movements::Knockback(TnuaBuiltinKnockback {
                shove: knockback_taken.value_for(knockback.length()).max(0.)
                    * knockback.normalize_or_zero(),
                force_forward: None,
            }))
        }

        // Hit stun: insert a fresh stun component so mobs skip decisions while stunned.
        commands.entity(entity).insert(HitStun::default());

        // Death handling.
        if health.0 <= 0. {
            dead.push(entity);
            if is_player {
                // Stop the corpse from sliding.
                controller.action_interrupt(Movements::Knockback(TnuaBuiltinKnockback {
                    shove: Vec3::ZERO,
                    force_forward: None,
                }));
                if **active_overlay == ActiveOverlay::LoadingScreen {
                    // Safety fallback (practically unreachable: mobs die with the
                    // old dimension): respawn in place immediately.
                    health.0 = max_health.value();
                    if let Some(spawn_point) = spawn_point {
                        local_transform.translation = spawn_point.0;
                    }
                } else {
                    // Death screen flow: the player stays dead (hidden, damage-immune,
                    // excluded from mob perception) until Respawn is pressed. Healing
                    // and the warp happen on respawn, so late damage cannot chip into
                    // a freshly restored health pool.
                    let in_battle = active_dimension
                        .as_ref()
                        .is_some_and(|dimension| dimension.key() != &*crate::dim::embers::LOBBY);
                    if in_battle {
                        // Extraction penalty: everything carried is lost.
                        if let Some(mut inventory) = inventory {
                            for slot in 0..inventory.size() {
                                if let Some(item) = inventory[slot].take() {
                                    commands.entity(item).despawn();
                                }
                            }
                        }
                    }
                    commands.entity(entity).insert((Dead, Visibility::Hidden));
                    next_overlay.set(ActiveOverlay::DeathScreen);
                }
            } else {
                // Drop loot and despawn.
                if let Some(loot_table) = loot_table {
                    for item_key in &loot_table.0 {
                        commands.spawn_scene(bsn! {
                            item_actor_of(item_stack(item_key.clone()))
                            template_value(Transform::from_translation(transform.translation() + Vec3::Y * 0.5))
                        });
                    }
                }
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Revives a player that is respawning: once they have actually landed in the
/// lobby (immediately for a lobby respawn, after the portal travel otherwise),
/// restore full health and re-show them. While this has not happened yet the
/// player is still dead, so no mob can hit them during the warp.
fn finalize_respawn(
    mut commands: Commands,
    mut players: Query<(Entity, &mut Health, &Attributes<MaxHealth>), (With<Player>, With<Dead>)>,
    active_dimension: Option<Single<&Dimension, With<ActiveDimension>>>,
    overlay: Res<State<ActiveOverlay>>,
) {
    for (entity, mut health, max_health) in &mut players {
        if **overlay == ActiveOverlay::LoadingScreen {
            continue;
        }
        if active_dimension
            .as_ref()
            .is_some_and(|dimension| dimension.key() == &*crate::dim::embers::LOBBY)
        {
            health.0 = max_health.value();
            // Re-insert (do NOT just remove) Visibility: bevy only propagates
            // InheritedVisibility for entities that still have a Visibility
            // component, so removing it would leave the player permanently
            // hidden after the corpse's Hidden state.
            commands
                .entity(entity)
                .remove::<Dead>()
                .insert(Visibility::Visible)
                .remove::<ai::HitStun>();
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Damage>()
        .add_message::<DamageNumber>()
        .init_resource::<SystemRng<SmallRng>>()
        .add_systems(
            Update,
            (damage, creeper::system, zombie::system, finalize_respawn)
                .run_if(in_state(crate::ui::GameState::Dimension)),
        )
        .add_plugins(ai::plugin)
        .add_plugins(damage_number::plugin)
        .add_plugins(attributes::plugin);
}
