use super::dim::DimensionViewNode;
use super::{ActiveOverlay, NodeInteraction, text, text_button};
use crate::dim::actor::living::player::{Player, SpawnPoint};
use crate::dim::{ActiveDimension, Dimension, embers};
use crate::ui::loading_screen::{DimensionEntryContext, Load};
use crate::utils::Keyed;
use avian3d::prelude::*;
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;

/// Starts the respawn: outside the lobby the player warps back to the lobby (the
/// extraction penalty), in the lobby they are teleported to their spawn point.
/// The player stays dead (hidden, damage-immune) until `finalize_respawn` (in
/// `dim::actor::living`) revives them once they are actually in the lobby, so no
/// mob can hit them during the warp's loading frames.
fn respawn_on_click(
    _event: On<NodeInteraction>,
    mut commands: Commands,
    mut next_overlay: ResMut<NextState<ActiveOverlay>>,
    player: Single<(Entity, &SpawnPoint), With<Player>>,
    active_dimension: Option<Single<&Dimension, With<ActiveDimension>>>,
) {
    let (entity, spawn_point) = player.into_inner();
    let in_battle = active_dimension.is_some_and(|dimension| dimension.key() != &*embers::LOBBY);
    if in_battle {
        commands.trigger(Load::EnterDimension(
            DimensionEntryContext::PortalTravel,
            embers::LOBBY.clone(),
        ));
    } else {
        commands.entity(entity).insert((
            Transform::from_translation(spawn_point.0),
            LinearVelocity::ZERO,
        ));
        next_overlay.set(ActiveOverlay::HeadsUpDisplay);
    }
}

fn init(mut commands: Commands, dimension_view_node: Single<Entity, With<DimensionViewNode>>) {
    commands.spawn_scene(bsn! {
        #DeathScreen
        ChildOf({*dimension_view_node})
        DespawnOnExit<ActiveOverlay>(ActiveOverlay::DeathScreen)
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
                #DeathScreenPanel
                Node {
                    width: px(320),
                    height: px(140),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceEvenly,
                    align_items: AlignItems::Center,
                }
                BackgroundColor(Color::srgba(0.25, 0.0, 0.0, 0.75))
                Children [
                    (text("You died", WHITE, 24.)),
                    (text_button("Respawn", respawn_on_click)),
                    (text("Press Respawn to continue", WHITE, 12.)),
                ]
            ),
        ]
    });
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(ActiveOverlay::DeathScreen), init);
}
