use std::{marker::PhantomData, time::Duration};

use bevy::{prelude::*, time::Stopwatch};

pub const ALERT_Z_INDEX: i32 = 1000;
pub const DEFAULT_ALERT_HEIGHT: f32 = 80.;

#[derive(Debug, Component)]
pub struct Alert {
    message: String,
}

impl Alert {
    pub fn bundle(message: impl Into<String>, lifetime: Duration) -> impl Bundle {
        (
            Self {
                message: message.into(),
            },
            Name::new("Alert"),
            AlertTimer {
                time_alive: Stopwatch::new(),
                lifetime,
            },
        )
    }
}

// Alert Plugin accepts one type parameter, M.
// This should implement Component and is used to allow multiple kinds
// of alert mechanisms to exist in parallel.
pub struct AlertsPlugin<M = AlertMarker> {
    marker: PhantomData<M>,
}

impl<M> Default for AlertsPlugin<M> {
    fn default() -> Self {
        Self {
            marker: PhantomData::<M>,
        }
    }
}

impl AlertsPlugin<AlertMarker> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn alert(
        In(alerts): In<Vec<String>>,
        mut commands: Commands,
        lifetime: Res<AlertLifetime<AlertMarker>>,
    ) {
        for alert in alerts {
            commands.spawn((Alert::bundle(alert, lifetime.lifetime), AlertMarker));
        }
    }
}

/// A default marker component for use with the default styles.
#[derive(Debug, Default, Component, Reflect)]
pub struct AlertMarker;

impl<M> AlertsPlugin<M> {
    /// Users can `pipe` their systems into this method
    pub fn custom_alert(
        In(alerts): In<Vec<String>>,
        mut commands: Commands,
        lifetime: Res<AlertLifetime<M>>,
    ) where
        M: Component + Default + TypePath + Send + Sync + 'static,
    {
        for alert in alerts {
            commands.spawn((Alert::bundle(alert, lifetime.lifetime), M::default()));
        }
    }
}

impl<M> Plugin for AlertsPlugin<M>
where
    M: Component + Default + TypePath + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<AlertElements<M>>()
            .insert_resource(AlertLifetime::<M>::new(Duration::from_secs(10)))
            .insert_resource(MaxAlerts::<M>::new(3))
            .add_systems(
                PostUpdate,
                (
                    Self::tick_active_alerts,
                    Self::despawn_alert_root,
                    Self::tick_transitions,
                    Self::spawn_alerts,
                    Self::handle_alert_button_bgs,
                    Self::handle_dismiss_alert_buttons,
                )
                    .chain()
                    .in_set(AlertSystems),
            );

        app.register_type::<AlertLifetime<M>>()
            .register_type::<MaxAlerts<M>>()
            .register_type::<AlertTimer>()
            .register_type::<AlertTransition>();
    }
}

impl<M> AlertsPlugin<M>
where
    M: Component + Default + TypePath,
{
    #[allow(clippy::type_complexity)]
    fn tick_active_alerts(
        mut commands: Commands,
        mut spawned_alerts: Query<(Entity, &mut AlertTimer), (With<M>, With<AlertUi>)>,
        time: Res<Time>,
    ) {
        for (entity, mut timer) in &mut spawned_alerts {
            timer.time_alive.tick(time.delta());
            if timer.time_alive.elapsed() > timer.lifetime {
                commands.entity(entity).insert(AlertTransition::FadeOut);
            }
        }
    }

    fn tick_transitions(
        mut commands: Commands,
        mut alert_nodes: Query<
            (
                Entity,
                &mut Style,
                &AlertTransition,
                Option<&mut TransitionTimer>,
            ),
            With<AlertUi>,
        >,
        time: Res<Time>,
    ) {
        for (entity, mut style, transition, timer) in &mut alert_nodes {
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
                AlertTransition::FadeIn => 1. - time,
                AlertTransition::FadeOut => time,
            });
            style.left = Val::Percent(left * 100.);

            if time >= 1. {
                match transition {
                    AlertTransition::FadeIn => {
                        commands
                            .entity(entity)
                            .remove::<(AlertTransition, TransitionTimer)>();
                    }
                    AlertTransition::FadeOut => {
                        commands.entity(entity).despawn_recursive();
                    }
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn despawn_alert_root(
        mut commands: Commands,
        spawned_alerts: Query<Entity, (With<M>, With<AlertUi>)>,
        alerts_to_spawn: Query<(Entity, &Alert), (With<M>, Without<AlertUi>)>,
        alerts_ui_root: Query<Entity, (With<M>, With<AlertUiRoot>)>,
    ) where
        M: Component + Send + Sync + 'static,
    {
        let num_live_alerts = spawned_alerts.iter().count();
        let num_unspawned_alerts = alerts_to_spawn.iter().count();

        // if there are no alerts, remove any containers
        if num_unspawned_alerts + num_live_alerts == 0 && !alerts_ui_root.is_empty() {
            // This is fine as long as this plugin guarantees to only create one root at a time.
            let entity = alerts_ui_root.single();
            commands.entity(entity).despawn_recursive();
        }
    }

    #[allow(clippy::type_complexity)]
    fn spawn_alerts(
        mut commands: Commands,
        spawned_alerts: Query<Entity, (With<M>, With<AlertUi>)>,
        alerts_to_spawn: Query<(Entity, &Alert), (With<M>, Without<AlertUi>)>,
        alerts_ui_root: Query<Entity, (With<M>, With<AlertUiRoot>)>,
        max_alerts: Res<MaxAlerts<M>>,
        alert_nodes: Res<AlertElements<M>>,
    ) where
        M: Component + Send + Sync + 'static,
    {
        let num_live_alerts = spawned_alerts.iter().count();
        let num_alert_spaces = max_alerts.saturating_sub(num_live_alerts);
        let num_unspawned_alerts = alerts_to_spawn.iter().count();

        if num_unspawned_alerts + num_live_alerts == 0 {
            return;
        }

        // if there are alerts and no root, add one first
        let root = if alerts_ui_root.is_empty() {
            // this is where we promise to only ever spawn one
            commands
                .spawn((
                    AlertUiRoot,
                    Name::new("Alert UI Root"),
                    NodeBundle {
                        z_index: ZIndex::Local(ALERT_Z_INDEX),
                        ..alert_nodes.container().clone()
                    },
                    M::default(),
                ))
                .id()
        } else {
            // otherwise get the root
            alerts_ui_root.single()
        };

        // spawn any alerts that we can
        for (entity, alert) in alerts_to_spawn.iter().take(num_alert_spaces) {
            let mut alert_node = alert_nodes.alert().clone();
            // set the left position to a 100% offset at first
            alert_node.style.left = Val::Percent(100.);
            commands
                .entity(entity)
                .insert((AlertUi, alert_node, AlertTransition::FadeIn, M::default()))
                .with_children(|builder| {
                    builder
                        .spawn((Name::new("Alert Header UI"), alert_nodes.header().clone()))
                        .with_children(|builder| {
                            builder
                                .spawn(AlertUi::dismiss_button(entity))
                                .with_children(|builder| {
                                    builder.spawn(AlertUi::dismiss_text());
                                });
                        });
                    builder
                        .spawn((Name::new("Alert Body UI"), alert_nodes.body().clone()))
                        .with_children(|builder| {
                            builder.spawn(AlertUi::text(
                                alert.message.clone(),
                                alert_nodes.text().clone(),
                            ));
                        });
                });
            commands.entity(root).add_child(entity);
        }
    }

    fn handle_alert_button_bgs(
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

    fn handle_dismiss_alert_buttons(
        mut commands: Commands,
        dismiss_buttons: Query<(&Interaction, &DismissButton)>,
    ) {
        for (interaction, button) in &dismiss_buttons {
            if matches!(interaction, Interaction::Pressed) {
                commands
                    .entity(button.alert)
                    .remove::<(AlertTransition, TransitionTimer)>();
                commands
                    .entity(button.alert)
                    .insert(AlertTransition::FadeOut);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub struct AlertSystems;

#[derive(Debug, Resource, Reflect)]
pub struct AlertLifetime<M: TypePath> {
    lifetime: Duration,
    #[reflect(ignore)]
    marker: PhantomData<M>,
}

impl<M> AlertLifetime<M>
where
    M: TypePath,
{
    pub fn new(lifetime: Duration) -> Self {
        AlertLifetime {
            lifetime,
            marker: PhantomData::<M>,
        }
    }
}

#[derive(Debug, Resource, Reflect)]
pub struct MaxAlerts<M: TypePath> {
    max: usize,
    #[reflect(ignore)]
    marker: PhantomData<M>,
}

impl<M> MaxAlerts<M>
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

impl<M> std::ops::Deref for MaxAlerts<M>
where
    M: TypePath,
{
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.max
    }
}

#[derive(Debug, Resource)]
pub struct AlertElements<M> {
    pub container: NodeBundle,
    pub alert: NodeBundle,
    pub header: NodeBundle,
    pub body: NodeBundle,
    pub text: TextStyle,
    pub marker: PhantomData<M>,
}

impl<M> AlertElements<M> {
    pub fn container(&self) -> &NodeBundle {
        &self.container
    }

    pub fn alert(&self) -> &NodeBundle {
        &self.alert
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

    pub fn corner_popup(alert_height: f32) -> Self {
        AlertElements {
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
            alert: NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    justify_content: JustifyContent::FlexStart,
                    width: Val::Percent(80.),
                    min_height: Val::Px(alert_height),
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

impl<M> Default for AlertElements<M> {
    fn default() -> Self {
        Self::corner_popup(DEFAULT_ALERT_HEIGHT)
    }
}

#[derive(Debug, Component, Reflect)]
pub struct AlertUiRoot;

#[derive(Debug, Component, Reflect)]
pub struct AlertTimer {
    time_alive: Stopwatch,
    lifetime: Duration,
}

#[derive(Clone, Debug, Component, Reflect)]
pub enum AlertTransition {
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
pub struct AlertUi;

impl AlertUi {
    fn text(message: String, style: TextStyle) -> impl Bundle {
        (
            Name::new("Alert Text"),
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
            DismissButton { alert: parent },
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
    alert: Entity,
}

#[cfg(test)]
mod tests {
    use bevy::time::TimeUpdateStrategy;

    use bevy_mod_try_system::TrySystemExt;

    use super::*;

    #[derive(Default, Component, Reflect)]
    struct MyAlert;

    fn alert_per_second(
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
            app.add_plugins(AlertsPlugin::<MyAlert>::default());
            app.add_systems(
                Update,
                alert_per_second.pipe_err(AlertsPlugin::<MyAlert>::custom_alert),
            );
        } else {
            app.add_plugins(AlertsPlugin::new());
            app.add_systems(Update, alert_per_second.pipe_err(AlertsPlugin::alert));
        }

        app
    }

    fn count_alerts(world: &mut World, use_custom: bool) -> usize {
        if use_custom {
            let mut query = world.query::<(&MyAlert, &Alert)>();
            query.iter(&world).count()
        } else {
            let mut query = world.query::<(&AlertMarker, &Alert)>();
            query.iter(&world).count()
        }
    }

    #[test]
    fn test_alert_ui() {
        for use_custom in [true, false] {
            let mut app = app(use_custom);
            // t: 0s
            app.update();
            // t: 0.25s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 0);
            app.update();
            // t: 0.5s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 0);
            app.update();
            // t: 0.75s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 0);
            app.update();
            // t: 1s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 0);
            app.update();
            // t: 1.25s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 1);
            app.update();
            // t: 1.5s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 1);
            app.update();
            // t: 1.75s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 1);
            app.update();
            // t: 2s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 1);
            app.update();
            // t: 2.25s
            let alerts = count_alerts(&mut app.world, use_custom);
            assert_eq!(alerts, 2);
            app.update();
        }
    }
}
