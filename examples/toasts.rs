use bevy::prelude::*;

use bevy_editor_pls::prelude::*;

use bevy_toasts::ToastPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugins(EditorPlugin::default().in_new_window(Window::default()));
    app.add_plugins(ToastPlugin::new());

    app.add_systems(Startup, init);
    app.add_systems(
        Update,
        fire_toast.pipe(ToastPlugin::toast).in_set(MySystems),
    );

    app.run();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(SystemSet)]
pub struct MySystems;

#[derive(Component)]
pub struct ToastButton;

fn init(mut commands: Commands) {
    commands.spawn((Camera2dBundle::default(), IsDefaultUiCamera));
    commands
        .spawn((
            Name::new("Banner"),
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..Default::default()
                },
                background_color: Color::ANTIQUE_WHITE.into(),
                ..Default::default()
            },
        ))
        .with_children(|builder| {
            builder.spawn(TextBundle::from_section(
                "Press Space to fire toast (or try F)",
                TextStyle {
                    font_size: 48.,
                    color: Color::BLACK,
                    ..Default::default()
                },
            ));
        });
}

fn fire_toast(inputs: Res<ButtonInput<KeyCode>>) -> Vec<String> {
    if inputs.just_pressed(KeyCode::Space) {
        vec!["Toast fired!".to_string()]
    } else if inputs.just_pressed(KeyCode::KeyF) {
        vec![
            "F! F! F! F! F! Very very long message! Very very long! So long! Super long message!"
                .to_string(),
        ]
    } else {
        vec![]
    }
}
