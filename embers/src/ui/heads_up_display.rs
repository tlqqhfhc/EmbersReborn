use super::ActiveOverlay;
use super::dim::DimensionViewNode;
use crate::dim::actor::living::Health;
use crate::dim::actor::living::attributes::{Attributes, MaxHealth};
use crate::dim::actor::living::player::{
    Player, PlayerInventory, SelectedHotbarSlot, process_input_hotbar_in_hud,
};
use crate::dim::item::ItemStack;
use crate::dim::item::inv::InventorySlot;
use crate::pld::foundry::{empty_image_node, item_image_node, ui_image_node};
use crate::utils::Keyed;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

#[derive(Clone, Component, Debug, Default)]
struct HotbarSlotNode(InventorySlot);

#[derive(Clone, Component, Default)]
struct HotbarSelectionIndicatorNode;

#[derive(Clone, Component, Default)]
struct MainHandSlotNode;

#[derive(Clone, Component, Default)]
struct HealthBarFillNode;

fn init(
    mut commands: Commands,
    dimension_view_node: Single<Entity, With<DimensionViewNode>>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    cursor_options.grab_mode = CursorGrabMode::Confined;
    fn hotbar_slot(slot: InventorySlot) -> impl Scene {
        bsn! {
            #HotbarSlot
            HotbarSlotNode(slot)
            Node {
                left: px(1),
                top: px(1),
                width: px(16),
                height: px(16),
                margin: px(2),
            }
            ImageNode
        }
    }
    commands.spawn_scene(bsn! {
        #HeadsUpDisplay
        ChildOf({*dimension_view_node})
        DespawnOnExit<ActiveOverlay>(ActiveOverlay::HeadsUpDisplay)
        Node {
            left: percent(50),
            bottom: px(7),
            margin: UiRect::left(px(-92.5)),
            position_type: PositionType::Absolute,
            width: px(185),
            height: px(18),
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        Children [
            (
                #Hotbar
                Node {
                    width: px(122),
                    height: px(22),
                    margin: UiRect::horizontal(px(3)),
                }
                ui_image_node("hud/hotbar")
                Children [
                    hotbar_slot(0),
                    hotbar_slot(1),
                    hotbar_slot(2),
                    hotbar_slot(3),
                    hotbar_slot(4),
                    hotbar_slot(5),
                    (
                        #HotbarSelectionIndicator
                        HotbarSelectionIndicatorNode
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(-1),
                            top: px(-1),
                            width: px(24),
                            height: px(23),
                        }
                        ui_image_node("hud/hotbar_selection")
                    ),
                ]
            ),
            (
                #MainHand
                Node {
                    width: px(22),
                    height: px(22),
                    margin: UiRect::horizontal(px(3)),
                }
                ui_image_node("hud/main_hand")
                Children [
                    (
                        #MainHandSlot
                        MainHandSlotNode
                        Node {
                            left: px(1),
                            top: px(1),
                            width: px(16),
                            height: px(16),
                            margin: px(2),
                        }
                        ImageNode
                    ),
                ]
            ),
            (
                #HealthBar
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50),
                    margin: UiRect::left(percent(-26)),
                    top: px(-14),
                    width: percent(52),
                    height: px(5),
                }
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.7))
                Children [
                    (
                        #HealthBarFillNode
                        HealthBarFillNode
                        Node {
                            width: percent(100),
                            height: percent(100),
                        }
                        BackgroundColor(Color::srgb(0.75, 0.1, 0.1))
                    ),
                ]
            ),
        ]
    });
}

fn fina(mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor_options.grab_mode = CursorGrabMode::None;
}

fn update_health_bar(
    player: Single<(&Health, &Attributes<MaxHealth>), With<Player>>,
    mut fill: Single<&mut Node, With<HealthBarFillNode>>,
) {
    let (health, max_health) = *player;
    let ratio = (health.0 / max_health.value()).clamp(0., 1.);
    fill.width = percent(ratio * 100.);
}

fn update_hotbar_selection_indicator(
    player: Single<Ref<SelectedHotbarSlot>, With<Player>>,
    mut hotbar_selection_node: Single<&mut Node, With<HotbarSelectionIndicatorNode>>,
) {
    let ref selected_hotbar_slot = *player;
    if selected_hotbar_slot.is_changed() {
        hotbar_selection_node.left = px(-1 + selected_hotbar_slot.0 * 20);
    }
}

fn update_hotbar(
    mut commands: Commands,
    player_inventory: Single<Ref<PlayerInventory>>,
    items: Query<&ItemStack>,
    main_hand_slot: Single<(Entity, Ref<MainHandSlotNode>)>,
    hotbar_slots: Query<(Entity, &HotbarSlotNode)>,
) {
    let (main_hand_slot_entity, main_hand_slot) = *main_hand_slot;
    if !player_inventory.is_changed() && !main_hand_slot.is_added() {
        return;
    }
    let mut main_hand_slot_commands = commands.entity(main_hand_slot_entity);
    match player_inventory.main_hand() {
        Some(item) => main_hand_slot_commands.apply_scene(item_image_node(
            items
                .get(item)
                .expect("Inventory held an item that doesn't exist")
                .key(),
        )),
        None => main_hand_slot_commands.apply_scene(empty_image_node()),
    };
    for (hotbar_slot_entity, hotbar_slot) in &hotbar_slots {
        let mut hotbar_slot_commands = commands.entity(hotbar_slot_entity);
        match player_inventory.hotbar(hotbar_slot.0) {
            Some(item) => hotbar_slot_commands.apply_scene(item_image_node(
                items
                    .get(item)
                    .expect("Inventory held an item that doesn't exist")
                    .key(),
            )),
            None => hotbar_slot_commands.apply_scene(empty_image_node()),
        };
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(ActiveOverlay::HeadsUpDisplay), init)
        .add_systems(OnExit(ActiveOverlay::HeadsUpDisplay), fina)
        .add_systems(Update, update_hotbar.after(process_input_hotbar_in_hud))
        .add_systems(
            Update,
            (update_hotbar_selection_indicator, update_health_bar),
        );
}
