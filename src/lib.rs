use std::{marker::PhantomData, time::Duration};

use bevy::{prelude::*, time::Stopwatch};

pub const TOAST_Z_INDEX: i32 = 1000;

#[derive(Default)]
#[derive(Component, Reflect)]
pub struct ToastMarker;

// Toast Plugin accepts one type parameter, M.
// This should implement Component and is used to allow multiple kinds
// of toast mechanisms to exist in parallel.
pub struct ToastPlugin<M = ToastMarker> {
    marker: PhantomData<M>,
}

impl<M> Default for ToastPlugin<M> {
    fn default() -> Self {
        Self {
            marker: PhantomData::<M>,
        }
    }
}

impl ToastPlugin<ToastMarker> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn toast(
        In(toasts): In<Vec<String>>,
        mut commands: Commands,
        lifetime: Res<ToastLifetime<ToastMarker>>,
    ) {
        for toast in toasts {
            commands.spawn((Toast::bundle(toast, lifetime.lifetime.clone()), ToastMarker));
        }
    }
}

impl<M> Plugin for ToastPlugin<M>
where
    M: Component + TypePath + Default + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<ToastElements<M>>()
            .insert_resource(ToastLifetime::<M>::new(Duration::from_secs(10)))
            .insert_resource(MaxToasts::<M>::new(3))
            .add_systems(
                PostUpdate,
                (
                    Self::tick_active_toasts,
                    Self::despawn_toast_root,
                    Self::spawn_toasts,
                    Self::handle_dismiss_toast_buttons,
                )
                    .chain()
                    .in_set(ToastSystems),
            );

        app.register_type::<ToastLifetime<M>>()
            .register_type::<MaxToasts<M>>()
            .register_type::<ToastTimer>();
    }
}

impl<M: Component + TypePath + Default> ToastPlugin<M> {
    /// Users can `pipe` their systems into this method
    pub fn custom_toast(
        In(toasts): In<Vec<String>>,
        mut commands: Commands,
        lifetime: Res<ToastLifetime<M>>,
    )
    // M: Send + Sync + 'static,
    {
        for toast in toasts {
            commands.spawn((
                Toast::bundle(toast, lifetime.lifetime.clone()),
                M::default(),
            ));
        }
    }

    fn tick_active_toasts(
        mut commands: Commands,
        mut spawned_toasts: Query<(Entity, &mut ToastTimer), (With<M>, With<ToastUi>)>,
        time: Res<Time>,
    ) {
        for (entity, mut timer) in spawned_toasts.iter_mut() {
            timer.time_alive.tick(time.delta());
            if timer.time_alive.elapsed() > timer.lifetime {
                // TODO: fade out?
                // commands.entity(entity).despawn_recursive();
            }
        }
    }

    fn despawn_toast_root(
        mut commands: Commands,
        spawned_toasts: Query<Entity, (With<M>, With<ToastUi>)>,
        toasts_to_spawn: Query<(Entity, &Toast), (With<M>, Without<ToastUi>)>,
        toasts_ui_root: Query<Entity, (With<M>, With<ToastUiRoot>)>,
    ) where
        M: Component + Send + Sync + 'static,
    {
        let num_live_toasts = spawned_toasts.iter().count();
        let num_unspawned_toasts = toasts_to_spawn.iter().count();

        // if there are no toasts, remove any containers
        if num_unspawned_toasts + num_live_toasts == 0 {
            if !toasts_ui_root.is_empty() {
                // This is fine as long as this plugin guarantees to only create one root at a time.
                let entity = toasts_ui_root.single();
                commands.entity(entity).despawn_recursive();
            }
            return;
        }
    }

    fn spawn_toasts(
        mut commands: Commands,
        spawned_toasts: Query<Entity, (With<M>, With<ToastUi>)>,
        toasts_to_spawn: Query<(Entity, &Toast), (With<M>, Without<ToastUi>)>,
        toasts_ui_root: Query<Entity, (With<M>, With<ToastUiRoot>)>,
        max_toasts: Res<MaxToasts<M>>,
        toast_nodes: Res<ToastElements<M>>,
    ) where
        M: Component + Send + Sync + 'static,
    {
        let num_live_toasts = spawned_toasts.iter().count();
        let num_toast_spaces = max_toasts.saturating_sub(num_live_toasts);
        let num_unspawned_toasts = toasts_to_spawn.iter().count();

        if num_unspawned_toasts + num_live_toasts == 0 {
            return;
        }

        // if there are toasts and no root, add one first
        let root = if toasts_ui_root.is_empty() {
            // this is where we promise to only ever spawn one
            commands
                .spawn((
                    ToastUiRoot,
                    Name::new("Toast UI Root"),
                    NodeBundle {
                        z_index: ZIndex::Local(TOAST_Z_INDEX),
                        ..toast_nodes.container().clone()
                    },
                    M::default(),
                ))
                .id()
        } else {
            // otherwise get the root
            toasts_ui_root.single()
        };

        // spawn any toasts that we can
        for (entity, toast) in toasts_to_spawn.iter().take(num_toast_spaces) {
            commands
                .entity(entity)
                .insert((ToastUi, toast_nodes.toast().clone(), M::default()))
                .with_children(|builder| {
                    builder
                        .spawn(ToastUi::dismiss_button(builder.parent_entity()))
                        .with_children(|builder| {
                            builder.spawn(ToastUi::dismiss_text());
                        });
                    builder.spawn(ToastUi::text(
                        toast.message.clone(),
                        toast_nodes.text().clone(),
                    ));
                });
            commands.entity(root).add_child(entity);
        }
    }

    fn handle_dismiss_toast_buttons(
        mut commands: Commands,
        dismiss_buttons: Query<(&Interaction, &DismissButton)>,
    ) {
        for (interaction, button) in &dismiss_buttons {
            if matches!(interaction, Interaction::Pressed) {
                commands.entity(button.toast).despawn_recursive();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(SystemSet)]
pub struct ToastSystems;

#[derive(Debug)]
#[derive(Resource, Reflect)]
pub struct ToastLifetime<M: TypePath> {
    lifetime: Duration,
    #[reflect(ignore)]
    marker: PhantomData<M>,
}

impl<M> ToastLifetime<M>
where
    M: TypePath,
{
    pub fn new(lifetime: Duration) -> Self {
        ToastLifetime {
            lifetime,
            marker: PhantomData::<M>,
        }
    }
}

#[derive(Debug)]
#[derive(Resource, Reflect)]
pub struct MaxToasts<M: TypePath> {
    max: usize,
    #[reflect(ignore)]
    marker: PhantomData<M>,
}

impl<M> MaxToasts<M>
where
    M: TypePath,
{
    pub fn new(max: usize) -> Self {
        Self {
            max,
            marker: PhantomData::<M>,
        }
    }
}

impl<M> std::ops::Deref for MaxToasts<M>
where
    M: TypePath,
{
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.max
    }
}

#[derive(Debug)]
#[derive(Resource)]
pub struct ToastElements<M> {
    container: NodeBundle,
    toast: NodeBundle,
    text: TextStyle,
    marker: PhantomData<M>,
}

impl<M> ToastElements<M> {
    pub fn new(container: NodeBundle, toast: NodeBundle, text: TextStyle) -> Self {
        Self {
            container,
            toast,
            text,
            marker: PhantomData::<M>,
        }
    }

    pub fn container(&self) -> &NodeBundle {
        &self.container
    }

    pub fn toast(&self) -> &NodeBundle {
        &self.toast
    }

    pub fn text(&self) -> &TextStyle {
        &self.text
    }

    pub fn corner_popup() -> Self {
        ToastElements::new(
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(70.),
                    right: Val::Px(24.),
                    bottom: Val::Px(24.),
                    max_height: Val::Percent(60.),
                    display: Display::Flex,
                    flex_direction: FlexDirection::ColumnReverse,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::FlexEnd,
                    row_gap: Val::Px(8.),
                    ..Default::default()
                },
                background_color: Color::rgba(0., 0., 0., 0.).into(),
                ..Default::default()
            },
            NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    justify_content: JustifyContent::FlexEnd,
                    border: UiRect::all(Val::Px(2.)),
                    width: Val::Percent(80.),
                    min_height: Val::Px(80.),
                    ..Default::default()
                },
                background_color: Color::ALICE_BLUE.into(),
                border_color: Color::DARK_GRAY.into(),
                ..Default::default()
            },
            TextStyle {
                font_size: 24.,
                color: Color::BLACK,
                ..Default::default()
            },
        )
    }

    pub fn modal_stack() -> Self {
        ToastElements::new(
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.),
                    right: Val::Px(0.),
                    bottom: Val::Px(0.),
                    left: Val::Px(0.),
                    ..Default::default()
                },
                background_color: Color::rgba(0., 0., 0., 0.2).into(),
                ..Default::default()
            },
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(50.),
                    right: Val::Percent(50.),
                    margin: UiRect {
                        left: Val::Percent(-50.),
                        top: Val::Percent(-50.),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            TextStyle {
                font_size: 24.,
                color: Color::BLACK,
                ..Default::default()
            },
        )
    }
}

impl<M> Default for ToastElements<M> {
    fn default() -> Self {
        Self::corner_popup()
    }
}

#[derive(Debug)]
#[derive(Component)]
pub struct Toast {
    message: String,
}

impl Toast {
    pub fn bundle(message: impl Into<String>, lifetime: Duration) -> impl Bundle {
        (
            Self {
                message: message.into(),
            },
            Name::new("Toast"),
            ToastTimer {
                time_alive: Stopwatch::new(),
                lifetime,
            },
        )
    }
}

#[derive(Debug)]
#[derive(Component, Reflect)]
pub struct ToastUiRoot;

#[derive(Debug)]
#[derive(Component, Reflect)]
pub struct ToastTimer {
    time_alive: Stopwatch,
    lifetime: Duration,
}

#[derive(Debug)]
#[derive(Component)]
pub struct ToastUi;

impl ToastUi {
    fn text(message: String, style: TextStyle) -> impl Bundle {
        (
            Name::new("Toast Text"),
            TextBundle::from_section(message, style),
        )
    }

    fn dismiss_button(parent: Entity) -> impl Bundle {
        (
            Name::new("Dismiss Button"),
            ButtonBundle {
                style: Style {
                    width: Val::Px(30.),
                    height: Val::Px(30.),
                    align_self: AlignSelf::FlexEnd,
                    ..Default::default()
                },
                background_color: Color::DARK_GRAY.into(),
                border_color: Color::GRAY.into(),
                ..Default::default()
            },
            DismissButton { toast: parent },
        )
    }

    fn dismiss_text() -> impl Bundle {
        (
            Name::new("Dismiss X Button"),
            TextBundle::from_section(
                "X",
                TextStyle {
                    font_size: 18.,
                    color: Color::WHITE,
                    ..Default::default()
                },
            ),
        )
    }
}

#[derive(Component)]
pub struct DismissButton {
    toast: Entity,
}

#[cfg(test)]
mod tests {
    use bevy::time::TimeUpdateStrategy;

    use bevy_mod_try_system::TrySystemExt;

    use super::*;

    #[derive(Default)]
    #[derive(Component, Reflect)]
    struct MyToast;

    fn toast_per_second(
        time: Res<Time>,
        mut stopwatch: Local<Stopwatch>,
    ) -> Result<(), Vec<String>> {
        stopwatch.tick(time.delta());
        if stopwatch.elapsed_secs() >= 1. {
            let elapsed = stopwatch.elapsed();
            stopwatch.set_elapsed(elapsed.saturating_sub(Duration::from_secs(1)));
            Err(vec!["Another two seconds passed!".to_string()])
        } else {
            Ok(())
        }
    }

    fn app(use_custom: bool) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            250,
        )));
        if use_custom {
            app.add_plugins(ToastPlugin::<MyToast>::default());
            app.add_systems(
                Update,
                toast_per_second.pipe_err(ToastPlugin::<MyToast>::custom_toast),
            );
        } else {
            app.add_plugins(ToastPlugin::new());
            app.add_systems(Update, toast_per_second.pipe_err(ToastPlugin::toast));
        }

        app
    }

    fn count_toasts(world: &mut World, use_custom: bool) -> usize {
        if use_custom {
            let mut query = world.query::<(&MyToast, &Toast)>();
            query.iter(&world).count()
        } else {
            let mut query = world.query::<(&ToastMarker, &Toast)>();
            query.iter(&world).count()
        }
    }

    #[test]
    fn test_toast_ui() {
        for use_custom in [true, false] {
            let mut app = app(use_custom);
            // t: 0s
            app.update();
            // t: 0.25s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 0);
            app.update();
            // t: 0.5s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 0);
            app.update();
            // t: 0.75s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 0);
            app.update();
            // t: 1s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 0);
            app.update();
            // t: 1.25s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 1);
            app.update();
            // t: 1.5s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 1);
            app.update();
            // t: 1.75s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 1);
            app.update();
            // t: 2s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 1);
            app.update();
            // t: 2.25s
            let toasts = count_toasts(&mut app.world, use_custom);
            assert_eq!(toasts, 2);
            app.update();
        }
    }
}
