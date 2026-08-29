//! Central balance table for the extraction loop (P5).
//!
//! Every gameplay number that phase-2 tuning may touch lives here, so
//! rebalancing is a one-file change. Crafting recipes stay in
//! `ui::crafting::RECIPES` (already a centralized table).

use bevy::math::Vec3;

/// Ember shards granted to a brand-new player when the game starts in the
/// lobby (the title-screen Init flow). Later lobby entries (gateway / portal
/// travel) teleport the existing player and grant nothing.
pub const INITIAL_EMBER_SHARDS: u8 = 8;

/// Seconds after entering the operation until the extraction portal appears.
pub const PORTAL_DELAY_SECS: f32 = 90.;
/// Where the extraction portal appears once its timer is up.
pub const PORTAL_SPAWN: Vec3 = Vec3::new(0., 0.5, 0.);

/// Mobs spawned per operation dimension generation.
pub const CREEPER_SPAWN_COUNT: usize = 3;
pub const ZOMBIE_SPAWN_COUNT: usize = 5;

/// Scattered item loot per operation generation.
pub const SCATTERED_SHARD_COUNT: usize = 8;
pub const SCATTERED_WEAPON_COUNT: usize = 2;

/// Mobs and items spawn within `±SPAWN_BOUND` of the origin and at least
/// `ENTRY_CLEAR_RADIUS` away from the dimension entry point.
pub const SPAWN_BOUND: f32 = 16.;
pub const ENTRY_CLEAR_RADIUS: f32 = 6.;

/// Zombie death drop: this many ember shards.
pub const ZOMBIE_LOOT_SHARD_COUNT: usize = 2;
/// Zombie spawn weapon ratio (sword, spear, bare hand) out of their sum.
pub const ZOMBIE_WEAPON_RATIO: (usize, usize, usize) = (4, 3, 3);
