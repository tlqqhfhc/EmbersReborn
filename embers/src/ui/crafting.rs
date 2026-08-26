use super::dim::DimensionViewNode;
use super::{ActiveOverlay, NodeInteraction, text, text_button};
use crate::dim::actor::living::player::{Player, PlayerInventory};
use crate::dim::item::item_stack;
use crate::dim::item::{ItemStack, StackCount, embers};
use crate::pld::foundry::item_image_node;
use crate::utils::{Keyed, NamespacedKey};
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use std::sync::LazyLock;

/// A single crafting recipe: trade `material_count` of the material item for
/// `product_count` of the product item.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub name: &'static str,
    pub material: NamespacedKey,
    pub material_count: u8,
    pub product: NamespacedKey,
    pub product_count: u8,
}

/// The full recipe table (MVP code constants, centralized for tuning).
pub static RECIPES: LazyLock<Vec<Recipe>> = LazyLock::new(|| {
    vec![
        Recipe {
            name: "Sword",
            material: embers::EMBER_SHARD.clone(),
            material_count: 8,
            product: embers::SWORD.clone(),
            product_count: 1,
        },
        Recipe {
            name: "Spear",
            material: embers::EMBER_SHARD.clone(),
            material_count: 12,
            product: embers::SPEAR.clone(),
            product_count: 1,
        },
        Recipe {
            name: "TNT",
            material: embers::EMBER_SHARD.clone(),
            material_count: 4,
            product: embers::TNT.clone(),
            product_count: 1,
        },
    ]
});

/// Request to open the crafting overlay; triggered by the crafting station interaction.
#[derive(Clone, Copy, Debug, Event)]
pub struct OpenCrafting;

fn open_crafting(_event: On<OpenCrafting>, mut next_overlay: ResMut<NextState<ActiveOverlay>>) {
    next_overlay.set(ActiveOverlay::Crafting);
}

/// Marker on the Craft button of a recipe row; holds the recipe index.
#[derive(Clone, Component, Copy, Debug, Default)]
struct CraftButton(usize);

/// Total number of `key` items held across the whole inventory.
fn count_item(
    inventory: &PlayerInventory,
    items: &Query<(&ItemStack, &StackCount)>,
    key: &NamespacedKey,
) -> u32 {
    inventory[..]
        .iter()
        .flatten()
        .copied()
        .filter_map(|item| items.get(item).ok())
        .filter(|(stack, _)| stack.key() == key)
        .map(|(_, count)| count.0 as u32)
        .sum()
}

/// Consume up to `count` items of `key` from the inventory, splitting stacks
/// first-fit in slot order. Returns how many were actually consumed.
fn consume_item(
    commands: &mut Commands,
    inventory: &mut PlayerInventory,
    items: &Query<(&ItemStack, &StackCount)>,
    key: &NamespacedKey,
    count: u32,
) -> u32 {
    let mut remaining = count;
    let mut plan: Vec<(i8, Entity, u8)> = Vec::new();
    for slot in 0..inventory.size() {
        if remaining == 0 {
            break;
        }
        let Some(item) = inventory[slot] else {
            continue;
        };
        let Ok((stack, stack_count)) = items.get(item) else {
            continue;
        };
        if stack.key() != key {
            continue;
        }
        let take = (stack_count.0 as u32).min(remaining);
        plan.push((slot, item, take as u8));
        remaining -= take;
    }
    let consumed = count - remaining;
    for (slot, item, take) in plan {
        let stack_count = items
            .get(item)
            .expect("A just counted item should exist")
            .1
            .0;
        if take == stack_count {
            commands.entity(item).despawn();
            inventory[slot] = None;
        } else {
            commands.entity(item).insert(StackCount(stack_count - take));
        }
    }
    consumed
}

/// Executes `recipe_index` against the player's inventory: verifies materials and
/// a free slot, consumes the materials, and inserts the product into the first
/// free slot.
fn craft(
    commands: &mut Commands,
    player: Entity,
    inventory: &mut PlayerInventory,
    items: &Query<(&ItemStack, &StackCount)>,
    recipe_index: usize,
) {
    let recipe = &RECIPES[recipe_index];
    if count_item(inventory, items, &recipe.material) < recipe.material_count as u32 {
        warn!("Not enough {} to craft {}", recipe.material, recipe.name);
        return;
    }
    let Some(slot) = (0..inventory.size()).find(|slot| inventory[*slot].is_none()) else {
        warn!("No free inventory slot to hold the crafted {}", recipe.name);
        return;
    };
    let consumed = consume_item(
        commands,
        inventory,
        items,
        &recipe.material,
        recipe.material_count as u32,
    );
    if consumed < recipe.material_count as u32 {
        warn!(
            "Crafting aborted: only consumed {} of {} {}",
            consumed, recipe.material_count, recipe.material
        );
        return;
    }
    let mut product = commands.spawn_scene(item_stack(recipe.product.clone()));
    if recipe.product_count != 1 {
        product.insert(StackCount(recipe.product_count));
    }
    product.insert(ChildOf(player));
    inventory[slot] = Some(product.id());
}

fn craft_on_click(
    event: On<NodeInteraction>,
    mut commands: Commands,
    buttons: Query<&CraftButton, Without<InteractionDisabled>>,
    mut player: Single<(Entity, &mut PlayerInventory), With<Player>>,
    items: Query<(&ItemStack, &StackCount)>,
) {
    let Some(&CraftButton(index)) = buttons.get(event.event_target()).ok() else {
        return;
    };
    let (player, ref mut inventory) = *player;
    craft(&mut commands, player, inventory, &items, index);
}

fn item_icon(key: &NamespacedKey) -> impl Scene {
    bsn! {
        Node {
            width: px(16),
            height: px(16),
            margin: px(4),
        }
        item_image_node(key)
    }
}

fn craft_row(index: usize) -> impl Scene {
    let recipe = &RECIPES[index];
    bsn! {
        #CraftRow
        Node {
            width: percent(100),
            height: px(28),
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        Children [
            (item_icon(&recipe.material)),
            (text(format!("x{}", recipe.material_count), WHITE, 14.)),
            (text("->", WHITE, 14.)),
            (item_icon(&recipe.product)),
            (text(recipe.name, WHITE, 14.)),
            (
                CraftButton(index)
                text_button("Craft", craft_on_click)
            ),
        ]
    }
}

fn init(mut commands: Commands, dimension_view_node: Single<Entity, With<DimensionViewNode>>) {
    commands.spawn_scene(bsn! {
        #CraftingPanel
        ChildOf({*dimension_view_node})
        DespawnOnExit<ActiveOverlay>(ActiveOverlay::Crafting)
        Node {
            position_type: PositionType::Absolute,
            left: percent(0),
            top: percent(0),
            width: percent(100),
            height: percent(100),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        Children [
            (
                #CraftingPanelInner
                Node {
                    width: px(380),
                    height: px(170),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceEvenly,
                    align_items: AlignItems::Center,
                }
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))
                Children [
                    (text("Crafting", WHITE, 20.)),
                    (craft_row(0)),
                    (craft_row(1)),
                    (craft_row(2)),
                    (text("Press Escape to close", WHITE, 12.)),
                ]
            ),
        ]
    });
}

/// Keeps each recipe's Craft button enabled exactly while the player can afford it
/// and has a free slot for the product.
fn update_craft_buttons(
    mut commands: Commands,
    buttons: Query<(Entity, &CraftButton, Has<InteractionDisabled>)>,
    inventory: Single<&PlayerInventory, With<Player>>,
    items: Query<(&ItemStack, &StackCount)>,
) {
    let inventory = *inventory;
    for (entity, &CraftButton(index), disabled) in buttons.iter() {
        let recipe = &RECIPES[index];
        let available = count_item(inventory, &items, &recipe.material)
            >= recipe.material_count as u32
            && (0..inventory.size()).any(|slot| inventory[slot].is_none());
        match (available, disabled) {
            (true, true) => {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(entity).insert(InteractionDisabled);
            }
            (true, false) | (false, true) => {}
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_observer(open_crafting)
        .add_systems(OnEnter(ActiveOverlay::Crafting), init)
        .add_systems(
            Update,
            update_craft_buttons.run_if(in_state(ActiveOverlay::Crafting)),
        );
}
