pub mod actor;
pub mod block;
mod chunk;
pub mod item;

use crate::balance;
use crate::input::InteractionTrigger;
use crate::pld::manager::{
    PayloadManager, inject_keyed_embers_payload_batch, resolve_handle, resolve_payload,
};
use crate::pld::{Payload, PayloadApp, Tag};
use crate::ui::RootNode;
use crate::ui::crafting::OpenCrafting;
use crate::ui::loading_screen::{DimensionEntryContext, Load};
use crate::utils::{Keyed, NamespacedKey, SystemRng};
use actor::Actor;
use actor::item_actor::item_actor_of;
use actor::living::creeper::creeper;
use actor::living::dummy::dummy;
use actor::living::player::{Player, PlayerInventory, SpawnPoint, player_scene};
use actor::living::zombie::{ZombieWeapon, zombie};
use actor::living::{Damage, DamageKnockback, DamageSource, LivingActor, player};
use avian3d::math::Quaternion;
use avian3d::prelude::*;
use avian3d::schedule::LastPhysicsTick;
use bevy::asset::{AssetPath, HandleTemplate};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::Mutable;
use bevy::ecs::query::QueryFilter;
use bevy::ecs::system::{StaticSystemParam, SystemParam};
use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;
use bevy::time::Stopwatch;
use bevy_sprinkles::asset::{Gradient, Range};
use bevy_sprinkles::prelude::*;
use bevy_tnua::builtins::{TnuaBuiltinCrouch, TnuaBuiltinDash, TnuaBuiltinKnockback};
use bevy_tnua::prelude::*;
use derive_where::derive_where;
use embers_macros::identify;
use item::embers::{EMBER_SHARD, SPEAR, SWORD};
use item::inv::{ItemDestination, ItemMoveQuantity, ItemSource, MoveItemCommandExt};
use item::{StackCount, item_stack};
use rand::RngExt;
use rand::rngs::SmallRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Neg;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

pub mod embers {
    macro_rules! dim {
        ($id: ident, $key: expr) => {
            pub static $id: std::sync::LazyLock<$crate::utils::NamespacedKey> =
                std::sync::LazyLock::new(|| $crate::utils::NamespacedKey::new_embers($key));
        };
    }
    dim!(ASSEMBLY_APEX, "assembly_apex");
    dim!(LOBBY, "lobby");
    dim!(OPERATION, "operation");
}

#[derive(Default, Resource)]
pub struct LoadedDimensions(pub HashMap<NamespacedKey, Entity>);

#[derive(Clone, Component, Default)]
#[require(Dimension)]
pub struct ActiveDimension;

#[derive(Clone, Component, Debug)]
pub struct Dimension(NamespacedKey);

static DEFAULT_DIMENSION_KEY: LazyLock<NamespacedKey> =
    LazyLock::new(|| NamespacedKey::new("_", "missingno"));

impl Default for Dimension {
    fn default() -> Self {
        warn!("An default dimension is used! This is likely an error.");
        Self(DEFAULT_DIMENSION_KEY.clone())
    }
}

impl Keyed for Dimension {
    fn key(&self) -> &NamespacedKey {
        &self.0
    }
}

#[derive(Debug, Event)]
pub struct DimensionGenerationRequest(NamespacedKey);

impl DimensionGenerationRequest {
    pub fn new(key: &impl Keyed) -> Self {
        Self(key.key().clone())
    }
}

#[derive(Clone, Component, Copy, Default)]
struct Ground;

fn dimension_spawn_point(dimension: &NamespacedKey) -> Vec3 {
    if *dimension == *embers::LOBBY {
        Vec3::new(0., 1., 0.)
    } else if *dimension == *embers::OPERATION {
        Vec3::new(0., 1., 18.)
    } else {
        Vec3::new(0., 1., 0.)
    }
}

fn dimension_lighting() -> impl Scene {
    bsn! {
        #Sol
        DirectionalLight
        template_value(Transform::from_translation(Vec3::ONE).looking_at(Vec3::ZERO, Vec3::Y))
    }
}

fn dimension_ground(size: f32) -> impl Scene {
    bsn! {
        Mesh3d(asset_value(Plane3d::default().mesh().size(size, size)))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::WHITE))
        { PhysicsPreset::Environment.physics(false) }
        Ground
        template_value(Collider::heightfield(vec![vec![0.0, 0.0], vec![0.0, 0.0]], Vec3::splat(size)))
    }
}

/// Perimeter walls that prevent dynamic bodies (player, mobs, items, projectiles) from
/// falling off the edge of the dimension ground. Four static cuboid Environment-layer
/// walls are placed on the ±X and ±Z edges of a `size × size` ground plane; the
/// ±Z walls are lengthened to cover the corners, so there are no gaps.
/// Note: `Collider::cuboid` takes FULL dimensions (same as the mesh), not half extents.
fn dimension_barrier(size: f32) -> impl Scene {
    const THICKNESS: f32 = 1.0;
    const HEIGHT: f32 = 5.0;
    let half = 0.5 * size;
    let wall_color = Color::srgba(0.3, 0.4, 0.8, 0.18);
    bsn! {
        Children [
            // +Z (north) wall (length = full width + 2 corners)
            (
                Mesh3d(asset_value(
                    Cuboid::new(size + 2. * THICKNESS, HEIGHT, THICKNESS).mesh().build(),
                ))
                MeshMaterial3d::<StandardMaterial>(asset_value(wall_color))
                { PhysicsPreset::Environment.physics(false) }
                Collider::cuboid(size + 2. * THICKNESS, HEIGHT, THICKNESS)
                template_value(Transform::from_xyz(0., 0.5 * HEIGHT, half + 0.5 * THICKNESS))
            ),
            // -Z (south) wall
            (
                Mesh3d(asset_value(
                    Cuboid::new(size + 2. * THICKNESS, HEIGHT, THICKNESS).mesh().build(),
                ))
                MeshMaterial3d::<StandardMaterial>(asset_value(wall_color))
                { PhysicsPreset::Environment.physics(false) }
                Collider::cuboid(size + 2. * THICKNESS, HEIGHT, THICKNESS)
                template_value(Transform::from_xyz(0., 0.5 * HEIGHT, -half - 0.5 * THICKNESS))
            ),
            // +X (east) wall (length = full width, fits inside the ±Z walls)
            (
                Mesh3d(asset_value(
                    Cuboid::new(THICKNESS, HEIGHT, size).mesh().build(),
                ))
                MeshMaterial3d::<StandardMaterial>(asset_value(wall_color))
                { PhysicsPreset::Environment.physics(false) }
                Collider::cuboid(THICKNESS, HEIGHT, size)
                template_value(Transform::from_xyz(half + 0.5 * THICKNESS, 0.5 * HEIGHT, 0.))
            ),
            // -X (west) wall
            (
                Mesh3d(asset_value(
                    Cuboid::new(THICKNESS, HEIGHT, size).mesh().build(),
                ))
                MeshMaterial3d::<StandardMaterial>(asset_value(wall_color))
                { PhysicsPreset::Environment.physics(false) }
                Collider::cuboid(THICKNESS, HEIGHT, size)
                template_value(Transform::from_xyz(-half - 0.5 * THICKNESS, 0.5 * HEIGHT, 0.))
            ),
        ]
    }
}

fn lobby_scene() -> impl Scene {
    bsn! {
        #Dimension
        Dimension({embers::LOBBY.clone()})
        ActiveDimension
        Transform
        Visibility
        Children [
            (
                dimension_lighting()
            ),
            (
                dimension_ground(20.)
            ),
            (
                dimension_barrier(20.)
            ),
            (
                gateway(&INTERACTION_GATEWAY_TO_OPERATION)
                Transform::from_xyz(0.0, 0.5, -5.0)
            ),
            (
                #Dummy
                dummy()
                Transform::from_xyz(5.0, 0.5, 0.0)
            ),
            (
                crafting_station()
                Transform::from_xyz(-5.0, 0.5, 0.0)
            ),
        ]
    }
}

/// (center, size) of the fixed cuboid obstacles in the operation arena
/// (layout, not balance — gameplay numbers live in `crate::balance`).
const OPERATION_OBSTACLES: [(Vec3, Vec3); 7] = [
    (Vec3::new(-9., 0.75, -6.), Vec3::new(2.5, 1.5, 2.5)),
    (Vec3::new(9., 0.75, -6.), Vec3::new(2.5, 1.5, 2.5)),
    (Vec3::new(-9., 0.75, 6.), Vec3::new(2.5, 1.5, 2.5)),
    (Vec3::new(9., 0.75, 6.), Vec3::new(2.5, 1.5, 2.5)),
    (Vec3::new(0., 0.75, -12.), Vec3::new(4., 2., 2.)),
    (Vec3::new(-5., 0.75, 12.), Vec3::new(2., 1.5, 2.)),
    (Vec3::new(5., 0.75, 12.), Vec3::new(2., 1.5, 2.)),
];

fn obstacle(center: Vec3, size: Vec3) -> impl Scene {
    bsn! {
        Mesh3d(asset_value(Cuboid::new(size.x, size.y, size.z).mesh().build()))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::srgb(0.45, 0.4, 0.35)))
        { PhysicsPreset::Environment.physics(false) }
        Collider::cuboid(size.x, size.y, size.z)
        template_value(Transform::from_translation(center))
    }
}

fn operation_scene() -> impl Scene {
    bsn! {
        #Dimension
        Dimension({embers::OPERATION.clone()})
        ActiveDimension
        Transform
        Visibility
        PortalTimer({Timer::from_seconds(balance::PORTAL_DELAY_SECS, TimerMode::Once)})
        Children [
            (
                dimension_lighting()
            ),
            (
                dimension_ground(40.)
            ),
            (
                dimension_barrier(40.)
            ),
            (
                obstacle(OPERATION_OBSTACLES[0].0, OPERATION_OBSTACLES[0].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[1].0, OPERATION_OBSTACLES[1].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[2].0, OPERATION_OBSTACLES[2].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[3].0, OPERATION_OBSTACLES[3].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[4].0, OPERATION_OBSTACLES[4].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[5].0, OPERATION_OBSTACLES[5].1)
            ),
            (
                obstacle(OPERATION_OBSTACLES[6].0, OPERATION_OBSTACLES[6].1)
            ),
        ]
    }
}

/// Countdown on the operation dimension root; when it is up, the extraction
/// portal (a gateway back to the lobby) spawns and stays.
#[derive(Component, Clone, Default, Debug)]
pub struct PortalTimer(Timer);

fn tick_portal_timers(
    mut commands: Commands,
    mut operations: Query<(Entity, &mut PortalTimer), (With<Dimension>, With<ActiveDimension>)>,
    time: Res<Time>,
) {
    for (entity, mut portal_timer) in operations.iter_mut() {
        portal_timer.0.tick(time.delta());
        if portal_timer.0.just_finished() {
            let mut portal = commands.spawn_scene(bsn! {
                gateway(&INTERACTION_GATEWAY_TO_LOBBY)
                template_value(Transform::from_translation(balance::PORTAL_SPAWN))
            });
            portal.insert(ChildOf(entity));
        }
    }
}

/// Random spawn position inside the operation arena, kept clear of the entry.
fn random_operation_position(rng: &mut SystemRng<SmallRng>) -> Vec3 {
    const ENTRY: Vec3 = Vec3::new(0., 1., 18.);
    for _ in 0..16 {
        let position = Vec3::new(
            rng.random_range(-balance::SPAWN_BOUND..=balance::SPAWN_BOUND),
            1.,
            rng.random_range(-balance::SPAWN_BOUND..=balance::SPAWN_BOUND),
        );
        if position.distance(ENTRY) > balance::ENTRY_CLEAR_RADIUS {
            return position;
        }
    }
    Vec3::new(0., 1., -balance::SPAWN_BOUND)
}

/// Spawns the operation's mobs and scattered item loot. Called when the
/// operation dimension is generated.
fn spawn_operation_content(commands: &mut Commands, rng: &mut SystemRng<SmallRng>) {
    for _ in 0..balance::CREEPER_SPAWN_COUNT {
        commands.spawn_scene(bsn! {
            creeper()
            template_value(Transform::from_translation(random_operation_position(rng)))
        });
    }
    for _ in 0..balance::ZOMBIE_SPAWN_COUNT {
        commands.spawn_scene(bsn! {
            zombie(ZombieWeapon::roll(rng))
            template_value(Transform::from_translation(random_operation_position(rng)))
        });
    }
    let scattered_items: Vec<NamespacedKey> = (0..balance::SCATTERED_SHARD_COUNT)
        .map(|_| EMBER_SHARD.clone())
        .chain(
            (0..balance::SCATTERED_WEAPON_COUNT).map(|_| match rng.random_range(0..2) {
                0 => SWORD.clone(),
                _ => SPEAR.clone(),
            }),
        )
        .collect();
    for item_key in scattered_items {
        commands.spawn_scene(bsn! {
            item_actor_of(item_stack(item_key))
            template_value(Transform::from_translation(random_operation_position(rng)))
        });
    }
}

fn unknown_dimension_scene(dimension: &NamespacedKey) -> impl Scene {
    warn!("No scene is registered for dimension {dimension}; spawning a minimal one");
    bsn! {
        #Dimension
        Dimension({dimension.clone()})
        ActiveDimension
        Transform
        Visibility
        Children [
            (
                dimension_lighting()
            ),
            (
                dimension_ground(20.)
            ),
        ]
    }
}

fn handle_dimension_generation_request(
    request: On<DimensionGenerationRequest>,
    mut commands: Commands,
    mut loaded: ResMut<LoadedDimensions>,
    players: Query<Entity, With<Player>>,
    root_node: Single<Entity, With<RootNode>>,
    mut rng: ResMut<SystemRng<SmallRng>>,
) {
    let DimensionGenerationRequest(key) = &*request;
    for (evicted, entity) in loaded.0.drain() {
        info!("Evicting dimension {evicted}");
        commands.entity(entity).despawn();
    }
    let new_dimension = if *key == *embers::LOBBY {
        commands.spawn_scene(lobby_scene()).id()
    } else if *key == *embers::OPERATION {
        commands.spawn_scene(operation_scene()).id()
    } else {
        commands.spawn_scene(unknown_dimension_scene(key)).id()
    };
    loaded.0.insert(key.clone(), new_dimension);
    if *key == *embers::OPERATION {
        spawn_operation_content(&mut commands, &mut rng);
    }
    let spawn_point = dimension_spawn_point(key);
    if let Some(player) = players.iter().next() {
        commands.entity(player).insert((
            Transform::from_translation(spawn_point),
            SpawnPoint(spawn_point),
            LinearVelocity::from(Vec3::ZERO),
        ));
    } else {
        let mut player_entity = commands.spawn_scene(player_scene(spawn_point));
        player_entity.insert(ChildOf(*root_node));
        let player_id = player_entity.id();
        // A brand-new player starting in the lobby (the title-screen Init
        // flow) gets the initial shard stash. Every other lobby entry
        // (gateway / portal travel) teleports the existing player instead
        // and grants nothing.
        if *key == *embers::LOBBY {
            grant_initial_stash(&mut commands, player_id);
        }
    }
}

/// Puts the initial `balance::INITIAL_EMBER_SHARDS` stack into the new
/// player's first inventory slot.
fn grant_initial_stash(commands: &mut Commands, player: Entity) {
    let shard = commands
        .spawn_scene(item_stack(EMBER_SHARD.clone()))
        .insert(StackCount(balance::INITIAL_EMBER_SHARDS))
        .id();
    commands.entity(shard).insert(ChildOf(player));
    commands.queue(move |world: &mut World| {
        if let Some(mut inventory) = world.get_mut::<PlayerInventory>(player) {
            inventory[0] = Some(shard);
        }
    });
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Direction {
    None,
    East,
    West,
    Up,
    Down,
    South,
    North,
}

impl Direction {
    #[inline]
    pub const fn is_cartesian(&self) -> bool {
        matches!(
            self,
            Self::East | Self::West | Self::Up | Self::Down | Self::South | Self::North
        )
    }
}

impl Neg for Direction {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        match self {
            Self::None => Self::None,
            Self::East => Self::West,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::South => Self::North,
            Self::North => Self::South,
        }
    }
}

impl From<Direction> for Vec3 {
    #[inline]
    fn from(value: Direction) -> Self {
        match value {
            Direction::None => Self::ZERO,
            Direction::East => Self::X,
            Direction::West => Self::NEG_X,
            Direction::Up => Self::Y,
            Direction::Down => Self::NEG_Y,
            Direction::North => Self::Z,
            Direction::South => Self::NEG_Z,
        }
    }
}

impl From<Direction> for IVec3 {
    #[inline]
    fn from(value: Direction) -> Self {
        match value {
            Direction::None => Self::ZERO,
            Direction::East => Self::X,
            Direction::West => Self::NEG_X,
            Direction::Up => Self::Y,
            Direction::Down => Self::NEG_Y,
            Direction::North => Self::Z,
            Direction::South => Self::NEG_Z,
        }
    }
}

const FREE: LockedAxes = LockedAxes::new();
const LOCK_XZ_ROTATION: LockedAxes = LockedAxes::new().lock_rotation_x().lock_rotation_z();

#[derive(PhysicsLayer, Default, Copy, Clone)]
enum CollisionLayer {
    Interactable,
    LivingActor,
    MiscActor,
    #[default]
    Phantom,
    Projectile,
    Environment,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum PhysicsPreset {
    LivingActor,
    MiscActor,
    Phantom,
    Projectile,
    Environment,
}

impl PhysicsPreset {
    #[inline]
    pub fn physics(&self, interactable: bool) -> impl Scene {
        bsn! {
            CollisionLayers {
                memberships: {LayerMask(
                    match self {
                        Self::LivingActor => CollisionLayer::LivingActor,
                        Self::MiscActor => CollisionLayer::MiscActor,
                        Self::Phantom => CollisionLayer::Phantom,
                        Self::Projectile => CollisionLayer::Projectile,
                        Self::Environment => CollisionLayer::Environment,
                    }
                    .to_bits()
                        | if interactable {
                            CollisionLayer::Interactable.to_bits()
                        } else {
                            0
                        },
                )},
                filters: {match self {
                    Self::Phantom => [
                        CollisionLayer::LivingActor,
                        CollisionLayer::MiscActor,
                        CollisionLayer::Projectile,
                        CollisionLayer::Environment,
                    ]
                    .into(),
                    Self::Environment => [
                        CollisionLayer::LivingActor,
                        CollisionLayer::MiscActor,
                        CollisionLayer::Phantom,
                        CollisionLayer::Projectile,
                    ]
                    .into(),
                    _ => LayerMask::ALL,
                }},
            }
            Dominance({match self {
                Self::LivingActor => 3,
                Self::MiscActor => 2,
                Self::Phantom => 0,
                Self::Projectile => 1,
                Self::Environment => 4,
            }})
            template_value(match self {
                Self::LivingActor => LOCK_XZ_ROTATION,
                _ => FREE,
            })
            template_value(match self {
                Self::Environment => RigidBody::Static,
                _ => RigidBody::Dynamic,
            })
        }
    }
}

pub fn exclude_source(source: Entity) -> impl Scene {
    bsn! {
        template(move |_| Ok(SourceExclusion(source, Tick::MAX)))
        ActiveCollisionHooks::MODIFY_CONTACTS
    }
}

#[derive(Component)]
#[component(storage = "SparseSet")] // TODO: Benchmark whether sparse set storage is actually more performant than table storage
struct SourceExclusion(Entity, Tick);

// TODO: Consider removing unused active collision hooks?

#[derive(SystemParam)]
pub(super) struct SourceExclusionCollisionHooks<'w, 's> {
    exclusions: Query<'w, 's, &'static SourceExclusion>,
    last_physics_tick: Res<'w, LastPhysicsTick>,
}

impl CollisionHooks for SourceExclusionCollisionHooks<'_, '_> {
    fn modify_contacts(&self, contacts: &mut ContactPair, commands: &mut Commands) -> bool {
        let mut exclude = |entity0, entity1| {
            if let Ok(exclusion) = self.exclusions.get(entity0) {
                if exclusion.1 == Tick::MAX
                    || self
                        .last_physics_tick
                        .0
                        .get()
                        .wrapping_sub(exclusion.1.get())
                        == 1
                {
                    if exclusion.0 == entity1 {
                        commands
                            .entity(entity0)
                            .insert(SourceExclusion(entity1, self.last_physics_tick.0));
                        return false;
                    }
                } else {
                    #[cfg(debug_assertions)]
                    if self.last_physics_tick.0.get() <= exclusion.1.get() {
                        warn!(
                            "Expected last exclusion tick ({}) to be smaller than current physics tick ({}).",
                            exclusion.1.get(),
                            self.last_physics_tick.0.get()
                        );
                    }
                    commands.entity(entity0).remove::<SourceExclusion>();
                }
            }
            true
        };
        exclude(contacts.collider1, contacts.collider2)
            && exclude(contacts.collider2, contacts.collider1)
    }
}

#[derive(TnuaScheme, Debug)]
#[scheme(basis = TnuaBuiltinWalk)]
pub enum Movements {
    Knockback(TnuaBuiltinKnockback),
    Sneak(TnuaBuiltinCrouch),
    Roll(TnuaBuiltinDash),
}

impl Payload for MovementsConfig {
    fn payload_root() -> AssetPath<'static> {
        "movements_configs".into()
    }
}

pub trait Action: Payload + Clone + Keyed {
    type Environment: SystemParam;
    fn on_begin<'world, 'state>(
        &self,
        environment: &mut StaticSystemParam<'world, 'state, Self::Environment>,
        object: Entity,
    );
    fn on_end<'world, 'state>(
        &self,
        environment: &mut StaticSystemParam<'world, 'state, Self::Environment>,
        object: Entity,
        duration: Option<Duration>,
    ) -> Option<NamespacedKey>;
    fn duration(&self) -> Duration;
}

#[derive(Eq, PartialEq, Clone)]
#[derive_where(Default)]
pub struct ActionSlots<A: Action> {
    click: Option<Handle<A>>,
    double_click: Option<Handle<A>>,
}

impl<A: Action> ActionSlots<A> {
    pub fn get(&self, trigger: InteractionTrigger) -> Option<&Handle<A>> {
        match trigger {
            InteractionTrigger::Click => self.click.as_ref(),
            InteractionTrigger::DoubleClick => self.double_click.as_ref(),
        }
    }
    pub fn set(&mut self, trigger: InteractionTrigger, action: Handle<A>) {
        match trigger {
            InteractionTrigger::Click => self.click = Some(action),
            InteractionTrigger::DoubleClick => self.double_click = Some(action),
        }
    }
    pub fn clear(&mut self, trigger: InteractionTrigger) {
        match trigger {
            InteractionTrigger::Click => self.click = None,
            InteractionTrigger::DoubleClick => self.double_click = None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum ActionStatus {
    #[default]
    Idle,
    Active {
        timer: Stopwatch,
        trigger: InteractionTrigger,
    },
}

impl ActionStatus {
    pub fn idle() -> Self {
        Self::Idle
    }
    pub fn activate(trigger: InteractionTrigger) -> Self {
        Self::Active {
            timer: Stopwatch::new(),
            trigger,
        }
    }
    #[inline]
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

pub trait ActionStatusComponent: Component<Mutability = Mutable> {
    type Key;
    fn get_action_status(&self, key: &Self::Key) -> &ActionStatus;
    fn get_action_status_mut(&mut self, key: &Self::Key) -> &mut ActionStatus;
}

pub trait ActionSlotsComponent<A: Action>: Component<Mutability = Mutable> {
    type Key;
    fn get_actions(&self, key: &Self::Key) -> &ActionSlots<A>;
    fn get_actions_mut(&mut self, key: &Self::Key) -> &mut ActionSlots<A>;
}

#[derive(Message)]
pub struct ActionInterruption {
    pub agent_entity: Entity,
    pub interruption: NamespacedKey,
}

fn update_action<
    A: Action<Environment = Env> + Send + Sync + 'static,
    Key: 'static,
    Status: ActionStatusComponent<Key = Key>,
    Slot: ActionSlotsComponent<A, Key = Key>,
    Filter: QueryFilter,
    Env: SystemParam + 'static,
>(
    (In(agent_entity), In(actions_key), In(mut trigger), In(object)): (
        In<Entity>,
        In<Key>,
        In<Option<InteractionTrigger>>,
        In<Option<Entity>>,
    ),
    mut agent: Query<(&mut Status, &mut Slot), Filter>,
    mut environment: StaticSystemParam<Env>,
    payload_manager: Res<PayloadManager>,
    asset_server: Res<AssetServer>,
    actions: Res<Assets<A>>,
    action_tags: Res<Assets<Tag<A>>>,
    mut interruptions: MessageReader<ActionInterruption>,
) {
    let (ref mut status, ref mut slots) = agent.get_mut(agent_entity).unwrap();
    let status = status.get_action_status_mut(&actions_key);
    let slots = slots.get_actions_mut(&actions_key);
    //let environment = environment.into_inner();
    trigger.take_if(|active_trigger| {
        let interrupted = interruptions.read().any(|event| {
            event.agent_entity == agent_entity
                && resolve_payload(
                    &payload_manager,
                    &asset_server,
                    &action_tags,
                    &event.interruption,
                )
                .is_some_and(|interruptable| {
                    interruptable.contains(
                        slots
                            .get(*active_trigger)
                            .and_then(|handle| actions.get(handle))
                            .expect("Should not be performing nonexistent action")
                            .key(),
                    )
                })
        });
        interruptions.clear();
        interrupted
    });
    match trigger {
        Some(active_trigger) => {
            if status.is_idle() {
                if let Some(action) = slots.get(active_trigger)
                    && let Some(action) = actions.get(action)
                {
                    *status = ActionStatus::activate(active_trigger);
                    action.on_begin(
                        &mut environment,
                        object.expect("Action should not be performed on a nonexistent object."),
                    );
                }
            } else if let ActionStatus::Active {
                ref timer,
                trigger: current_trigger,
            } = *status
            {
                if let Some(action) = slots.get(active_trigger)
                    && let Some(action) = actions.get(action)
                {
                    let finished = timer.elapsed() >= action.duration();
                    if finished || active_trigger != current_trigger {
                        if let Some(new_action) = action
                            .on_end(
                                &mut environment,
                                object.expect(
                                    "Action should not be performed on a nonexistent object.",
                                ),
                                if finished {
                                    None
                                } else {
                                    Some(timer.elapsed())
                                },
                            )
                            .and_then(|new_action| {
                                resolve_handle(
                                    &payload_manager,
                                    &asset_server,
                                    &actions,
                                    &new_action,
                                )
                            })
                        {
                            slots.set(active_trigger, new_action);
                        }
                        *status = ActionStatus::activate(active_trigger);
                        action.on_begin(
                            &mut environment,
                            object
                                .expect("Action should not be performed on a nonexistent object."),
                        );
                    }
                } else if slots.get(current_trigger).is_none() {
                    *status = ActionStatus::idle();
                }
            }
        }
        None => {
            if let ActionStatus::Active { ref timer, trigger } = *status {
                if let Some(action) = slots.get(trigger)
                    && let Some(action) = actions.get(action)
                {
                    if let Some(new_action) = action
                        .on_end(
                            &mut environment,
                            object
                                .expect("Action should not be performed on a nonexistent object."),
                            Some(timer.elapsed()).filter(|used| action.duration() >= *used),
                        )
                        .and_then(|new_action| {
                            resolve_handle(&payload_manager, &asset_server, &actions, &new_action)
                        })
                    {
                        slots.set(trigger, new_action);
                    }
                }
                *status = ActionStatus::idle();
            }
        }
    }
}

#[derive(SystemParam)]
pub struct EntityInteractionEnvironment<'w, 's> {
    commands: Commands<'w, 's>,
    player: Single<'w, 's, (Entity, &'static PlayerInventory, &'static Transform), With<Player>>,
}

pub type EntityInteractionSlots = ActionSlots<EntityInteraction>;

#[derive(Asset, Clone, TypePath)]
#[identify(key)]
// TODO inspect do we need to clone this?
pub struct EntityInteraction {
    key: NamespacedKey,
    on_begin: Arc<dyn Fn(&mut EntityInteractionEnvironment, Entity) + Send + Sync>,
    on_end: Arc<
        dyn Fn(&mut EntityInteractionEnvironment, Entity, Option<Duration>) -> Option<NamespacedKey>
            + Send
            + Sync,
    >,
    duration: Duration,
}

impl Payload for EntityInteraction {
    fn payload_root() -> AssetPath<'static> {
        "entity_interactions".into()
    }
}

impl EntityInteraction {
    pub fn new(
        key: NamespacedKey,
        on_begin: impl Fn(&mut EntityInteractionEnvironment, Entity) + Send + Sync + 'static,
        on_end: impl Fn(
            &mut EntityInteractionEnvironment,
            Entity,
            Option<Duration>,
        ) -> Option<NamespacedKey>
        + Send
        + Sync
        + 'static,
        duration: Duration,
    ) -> Self {
        Self {
            key,
            on_begin: Arc::new(on_begin),
            on_end: Arc::new(on_end),
            duration,
        }
    }
}

impl Keyed for EntityInteraction {
    fn key(&self) -> &NamespacedKey {
        &self.key
    }
}

impl Action for EntityInteraction {
    type Environment = EntityInteractionEnvironment<'static, 'static>;
    fn on_begin(&self, environment: &mut StaticSystemParam<Self::Environment>, entity: Entity) {
        (self.on_begin)(environment, entity)
    }
    fn on_end(
        &self,
        environment: &mut StaticSystemParam<Self::Environment>,
        entity: Entity,
        duration: Option<Duration>,
    ) -> Option<NamespacedKey> {
        (self.on_end)(environment, entity, duration)
    }
    fn duration(&self) -> Duration {
        self.duration
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Interactable {
    /// The larger this is, the closer you need to get to interact
    pub distance_factor: f32,
    pub initial_click: Option<NamespacedKey>,
    pub initial_double_click: Option<NamespacedKey>,
}

impl Interactable {
    pub fn get_initial_interaction(&self, trigger: InteractionTrigger) -> Option<&NamespacedKey> {
        match trigger {
            InteractionTrigger::Click => self.initial_click.as_ref(),
            InteractionTrigger::DoubleClick => self.initial_double_click.as_ref(),
        }
    }
}

#[derive(Event)]
struct Explosion {
    pub source: DamageSource,
    pub power: f32,
}

fn explode(
    explosion: On<Explosion>,
    mut commands: Commands,
    spatial_query: SpatialQuery,
    living_actors: Query<(Entity, &GlobalTransform), With<LivingActor>>,
    mut damages: MessageWriter<Damage>,
) {
    let Explosion { source, power } = *explosion;
    #[derive(Default)]
    struct TmpParticles3dTemplate(HandleTemplate<ParticlesAsset>);
    impl Template for TmpParticles3dTemplate {
        type Output = Particles3d;
        fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
            Ok(Particles3d(self.0.build_template(context)?))
        }
        fn clone_template(&self) -> Self {
            Self(self.0.clone_template())
        }
    }
    commands.spawn_scene(bsn! {
        template_value(TmpParticles3dTemplate(asset_value(ParticlesAsset::new(
            "Huge Explosion".into(),
            ParticlesDimension::D3,
            default(),
            vec![
                EmitterData {
                    name: "explosion_burst".to_string(),
                    time: EmitterTime {
                        lifetime: 0.5,
                        lifetime_randomness: 0.4,
                        one_shot: true,
                        explosiveness: 1.0,
                        ..default()
                    },
                    draw_pass: EmitterDrawPass {
                        mesh: ParticleMesh::Quad {
                            orientation: default(),
                            size: Vec2::ONE,
                            subdivide: Vec2::ZERO,
                        },
                        material: DrawPassMaterial::Standard(StandardParticleMaterial {
                            alpha_mode: SerializableAlphaMode::Blend,
                            base_color_texture: Some(TextureRef::Asset(
                                "global/textures/particles/embers/explosion_10.png".to_string(),
                            )),
                            unlit: true,
                            ..default()
                        }),
                        transform_align: Some(TransformAlign::Billboard),
                        ..default()
                    },
                    emission: EmitterEmission {
                        shape: EmissionShape::Box { extents: Vec3::new(8., 8., 8.) },
                        particles_amount: 6,
                        ..default()
                    },
                    accelerations: EmitterAccelerations {
                        gravity: Vec3::new(0.0, 0.0, 0.0),
                        ..default()
                    },
                    scale: EmitterScale {
                        range: Range::new(0.1, 0.7),
                        ..default()
                    },
                    colors: EmitterColors {
                        initial_color: SolidOrGradientColor::Gradient { gradient: Gradient {
                            stops: vec![
                                GradientStop { color: [0.6, 0.6, 0.6, 1.0], position: 0.0 },
                                GradientStop { color: [1.0, 1.0, 1.0, 1.0], position: 1.0 },
                            ],
                            ..default()
                        } },
                        color_over_lifetime: Gradient::white(),
                        ..default()
                    },
                    ..default()
                },
            ],
            vec![],
            true,
            ParticlesAuthors {
                submitted_by: "TransparentWhite".to_string(),
                ..default()
            },
        ))))
        Transform::from_translation(source.origin)
    });
    // TODO consider more realistic explosions
    let radius = power * 2.;
    damages.write_batch(
        spatial_query
            .shape_intersections(
                &Collider::sphere(radius),
                source.origin,
                Quaternion::IDENTITY,
                &SpatialQueryFilter::from_mask(CollisionLayer::LivingActor),
            )
            .into_iter()
            .filter_map(|entity| living_actors.get(entity).ok())
            // The exploding creeper is still a living actor with a collider when
            // this query runs; damaging it would leave a stale message (it despawns
            // itself in the same frame).
            .filter(|&(entity, _)| Some(entity) != source.direct_entity)
            .map(|(entity, transform)| {
                let (direction, distance) =
                    (transform.translation() - source.origin).normalize_and_length();
                let x = 1. - distance / radius;
                Damage {
                    target: entity,
                    amount: 7. * (x * x + x) * power + 1.,
                    knockback: DamageKnockback::Directional(direction * x * 17.),
                    source,
                }
            }),
    );
}

/// Time of the day, within [0, 1).
#[derive(Component)]
pub struct WorldTime(pub f32);

impl Default for WorldTime {
    fn default() -> Self {
        Self(0.25)
    }
}

#[derive(Clone, Component, Default)]
struct Gateway;

pub static INTERACTION_GATEWAY_TO_LOBBY: LazyLock<NamespacedKey> =
    LazyLock::new(|| NamespacedKey::new_embers("gateway_to_lobby"));

pub static INTERACTION_GATEWAY_TO_OPERATION: LazyLock<NamespacedKey> =
    LazyLock::new(|| NamespacedKey::new_embers("gateway_to_operation"));

pub static INTERACTION_CRAFTING: LazyLock<NamespacedKey> =
    LazyLock::new(|| NamespacedKey::new_embers("crafting_open"));

/// A small crafting table. Interacting with it opens the crafting overlay.
/// Note: `Collider::cuboid` takes FULL dimensions (same as the mesh), not half extents.
fn crafting_station() -> impl Scene {
    bsn! {
        #CraftingStation
        Actor
        Mesh3d(asset_value(Cuboid::new(2., 1., 1.).mesh().build()))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::srgb(0.55, 0.42, 0.26)))
        { PhysicsPreset::Environment.physics(true) }
        Collider::cuboid(2., 1., 1.)
        Interactable {
            distance_factor: 1.,
            initial_click: { Some(INTERACTION_CRAFTING.clone()) },
            initial_double_click: None,
        }
    }
}

pub fn gateway(interaction: &NamespacedKey) -> impl Scene {
    bsn! {
        #Gateway
        Gateway
        Actor
        Mesh3d(asset_value(Cuboid::new(3., 1., 3.).mesh().build()))
        MeshMaterial3d<StandardMaterial>(asset_value(StandardMaterial {
            base_color: Color::BLACK,
            unlit: true,
            ..default()
        }))
        { PhysicsPreset::Phantom.physics(true) }
        template_value(RigidBody::Static)
        Collider::cuboid(1.5, 0.5, 1.5)
        Interactable {
            distance_factor: 1.,
            initial_click: { Some(interaction.clone()) },
            initial_double_click: None,
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<ActionInterruption>()
        .init_asset::<EntityInteraction>()
        .init_tags::<EntityInteraction>()
        .add_systems(
            PreStartup,
            inject_keyed_embers_payload_batch::<EntityInteraction>(
                "{}",
                [
                    EntityInteraction::new(
                        NamespacedKey::new_embers("item_actor/pickup"),
                        |_environment, _entity| {},
                        |EntityInteractionEnvironment { commands, player }, entity, _duration| {
                            let (player, inventory, _global_transform) = **player;
                            commands.move_item(
                                ItemSource::item_actor(entity),
                                ItemDestination::inventory_range(
                                    player,
                                    0..inventory.size(),
                                    inventory,
                                ),
                                ItemMoveQuantity::All,
                            );
                            None
                        },
                        Duration::from_millis(200),
                    ),
                    EntityInteraction::new(
                        INTERACTION_GATEWAY_TO_LOBBY.clone(),
                        |environment, _entity| {
                            environment.commands.trigger(Load::EnterDimension(
                                DimensionEntryContext::GatewayTravel,
                                embers::LOBBY.clone(),
                            ));
                        },
                        |_environment, _entity, _duration| None,
                        Duration::from_millis(200),
                    ),
                    EntityInteraction::new(
                        INTERACTION_GATEWAY_TO_OPERATION.clone(),
                        |environment, _entity| {
                            environment.commands.trigger(Load::EnterDimension(
                                DimensionEntryContext::GatewayTravel,
                                embers::OPERATION.clone(),
                            ));
                        },
                        |_environment, _entity, _duration| None,
                        Duration::from_millis(200),
                    ),
                    EntityInteraction::new(
                        INTERACTION_CRAFTING.clone(),
                        |environment, _entity| {
                            environment.commands.trigger(OpenCrafting);
                        },
                        |_environment, _entity, _duration| None,
                        Duration::from_millis(200),
                    ),
                ],
            ),
        )
        .init_resource::<LoadedDimensions>()
        .add_systems(
            Update,
            tick_portal_timers.run_if(in_state(crate::ui::GameState::Dimension)),
        )
        .add_observer(handle_dimension_generation_request)
        .add_observer(explode)
        .add_plugins(actor::plugin)
        .add_plugins(block::plugin)
        .add_plugins(item::plugin)
        .add_plugins(player::plugin);
}
