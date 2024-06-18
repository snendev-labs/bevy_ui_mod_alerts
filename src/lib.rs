use std::{marker::PhantomData, time::Duration};

use bevy::{prelude::*, time::Stopwatch};

pub const TOAST_Z_INDEX: i32 = 1000;
pub const DEFAULT_TOAST_HEIGHT: f32 = 80.;

#[derive(Default, Component, Reflect)]
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
            commands.spawn((Toast::bundle(toast, lifetime.lifetime), ToastMarker));
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
                    Self::tick_transitions,
                    Self::spawn_toasts,
                    Self::handle_toast_button_bgs,
                    Self::handle_dismiss_toast_buttons,
                )
                    .chain()
                    .in_set(ToastSystems),
            );

        app.register_type::<ToastLifetime<M>>()
            .register_type::<MaxToasts<M>>()
            .register_type::<ToastTimer>()
            .register_type::<ToastTransition>();
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
            commands.spawn((Toast::bundle(toast, lifetime.lifetime), M::default()));
        }
    }

    #[allow(clippy::type_complexity)]
    fn tick_active_toasts(
        mut commands: Commands,
        mut spawned_toasts: Query<(Entity, &mut ToastTimer), (With<M>, With<ToastUi>)>,
        time: Res<Time>,
    ) {
        for (entity, mut timer) in &mut spawned_toasts {
            timer.time_alive.tick(time.delta());
            if timer.time_alive.elapsed() > timer.lifetime {
                commands.entity(entity).insert(ToastTransition::FadeOut);
            }
        }
    }

    fn tick_transitions(
        mut commands: Commands,
        mut toast_nodes: Query<
            (
                Entity,
                &mut Style,
                &ToastTransition,
                Option<&mut TransitionTimer>,
            ),
            With<ToastUi>,
        >,
        time: Res<Time>,
    ) {
        for (entity, mut style, transition, timer) in &mut toast_nodes {
            let time = if let Some(mut timer) = timer {
                timer.tick(time.delta());
                timer.get_completion()
            } else {
                let mut timer = TransitionTimer::default();
                timer.tick(time.delta());
                let time = timer.get_completion();
                commands.entity(entity).insert(timer);
                time
            };

            fn ease(t: f32) -> f32 {
                if t > 1. {
                    1.
                } else if t < 0. {
                    0.
                } else {
                    1. - (std::f32::consts::PI * t).cos()
                }
            }

            let left = ease(match transition {
                ToastTransition::FadeIn => 1. - time,
                ToastTransition::FadeOut => time,
            });
            style.left = Val::Percent(left * 100.);

            if time >= 1. {
                match transition {
                    ToastTransition::FadeIn => {
                        commands
                            .entity(entity)
                            .remove::<(ToastTransition, TransitionTimer)>();
                    }
                    ToastTransition::FadeOut => {
                        commands.entity(entity).despawn_recursive();
                    }
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
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
        if num_unspawned_toasts + num_live_toasts == 0 && !toasts_ui_root.is_empty() {
            // This is fine as long as this plugin guarantees to only create one root at a time.
            let entity = toasts_ui_root.single();
            commands.entity(entity).despawn_recursive();
        }
    }

    #[allow(clippy::type_complexity)]
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
            let mut toast_node = toast_nodes.toast().clone();
            // set the left position to a 100% offset at first
            toast_node.style.left = Val::Percent(100.);
            commands
                .entity(entity)
                .insert((ToastUi, toast_node, ToastTransition::FadeIn, M::default()))
                .with_children(|builder| {
                    builder
                        .spawn((Name::new("Toast Header UI"), toast_nodes.header().clone()))
                        .with_children(|builder| {
                            builder
                                .spawn(ToastUi::dismiss_button(entity))
                                .with_children(|builder| {
                                    builder.spawn(ToastUi::dismiss_text());
                                });
                        });
                    builder
                        .spawn((Name::new("Toast Body UI"), toast_nodes.body().clone()))
                        .with_children(|builder| {
                            builder.spawn(ToastUi::text(
                                toast.message.clone(),
                                toast_nodes.text().clone(),
                            ));
                        });
                });
            commands.entity(root).add_child(entity);
        }
    }

    fn handle_toast_button_bgs(
        mut dismiss_buttons: Query<(&Interaction, &mut BackgroundColor), With<DismissButton>>,
    ) {
        for (interaction, mut bg_color) in &mut dismiss_buttons {
            bg_color.0 = match interaction {
                Interaction::Pressed => Color::DARK_GRAY,
                Interaction::Hovered => Color::rgb(0.4, 0.4, 0.4),
                Interaction::None => Color::rgb(0.35, 0.35, 0.35),
            };
        }
    }

    fn handle_dismiss_toast_buttons(
        mut commands: Commands,
        dismiss_buttons: Query<(&Interaction, &DismissButton)>,
    ) {
        for (interaction, button) in &dismiss_buttons {
            if matches!(interaction, Interaction::Pressed) {
                commands
                    .entity(button.toast)
                    .remove::<(ToastTransition, TransitionTimer)>();
                commands
                    .entity(button.toast)
                    .insert(ToastTransition::FadeOut);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub struct ToastSystems;

#[derive(Debug, Resource, Reflect)]
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

#[derive(Debug, Resource, Reflect)]
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

#[derive(Debug, Resource)]
pub struct ToastElements<M> {
    pub container: NodeBundle,
    pub toast: NodeBundle,
    pub header: NodeBundle,
    pub body: NodeBundle,
    pub text: TextStyle,
    pub marker: PhantomData<M>,
}

impl<M> ToastElements<M> {
    pub fn container(&self) -> &NodeBundle {
        &self.container
    }

    pub fn toast(&self) -> &NodeBundle {
        &self.toast
    }

    pub fn header(&self) -> &NodeBundle {
        &self.header
    }

    pub fn body(&self) -> &NodeBundle {
        &self.body
    }

    pub fn text(&self) -> &TextStyle {
        &self.text
    }

    pub fn corner_popup(toast_height: f32) -> Self {
        ToastElements {
            container: NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(70.),
                    right: Val::Px(24.),
                    bottom: Val::Px(24.),
                    max_height: Val::Percent(60.),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::FlexEnd,
                    row_gap: Val::Px(8.),
                    ..Default::default()
                },
                background_color: Color::rgba(0., 0., 0., 0.).into(),
                ..Default::default()
            },
            toast: NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    justify_content: JustifyContent::FlexStart,
                    width: Val::Percent(80.),
                    min_height: Val::Px(toast_height),
                    border: UiRect::all(Val::Px(2.)),
                    ..Default::default()
                },
                background_color: Color::ALICE_BLUE.into(),
                border_color: Color::DARK_GRAY.into(),
                ..Default::default()
            },
            header: NodeBundle {
                style: Style {
                    justify_content: JustifyContent::FlexEnd,
                    width: Val::Percent(100.),
                    height: Val::Px(20.),
                    ..Default::default()
                },
                background_color: Color::rgba(0., 0.8, 0.8, 0.8).into(),
                ..Default::default()
            },
            body: NodeBundle {
                style: Style {
                    flex_grow: 1.,
                    padding: UiRect::all(Val::Px(4.)),
                    width: Val::Percent(100.),
                    ..Default::default()
                },
                ..Default::default()
            },
            text: TextStyle {
                font_size: 24.,
                color: Color::BLACK,
                ..Default::default()
            },
            marker: Default::default(),
        }
    }
}

impl<M> Default for ToastElements<M> {
    fn default() -> Self {
        Self::corner_popup(DEFAULT_TOAST_HEIGHT)
    }
}

#[derive(Debug, Component)]
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

#[derive(Debug, Component, Reflect)]
pub struct ToastUiRoot;

#[derive(Debug, Component, Reflect)]
pub struct ToastTimer {
    time_alive: Stopwatch,
    lifetime: Duration,
}

#[derive(Clone, Debug, Component, Reflect)]
pub enum ToastTransition {
    FadeIn,
    FadeOut,
}

#[derive(Debug, Default, Component, Reflect)]
pub struct TransitionTimer {
    time_alive: Stopwatch,
}

impl TransitionTimer {
    pub const DURATION: Duration = Duration::from_millis(500);

    fn get_completion(&self) -> f32 {
        (self.time_alive.elapsed().as_secs_f32() / Self::DURATION.as_secs_f32())
            .max(0.)
            .min(1.)
    }

    fn tick(&mut self, delta: Duration) {
        self.time_alive.tick(delta);
    }
}

#[derive(Debug, Component)]
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
                    width: Val::Px(22.),
                    height: Val::Percent(100.),
                    padding: UiRect::px(2., 2., 2., 4.),
                    align_self: AlignSelf::FlexEnd,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                background_color: Color::DARK_GRAY.into(),
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

    #[derive(Default, Component, Reflect)]
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
