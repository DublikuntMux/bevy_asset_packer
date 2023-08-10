use bevy::{prelude::*, log::{LogPlugin, Level}};
use bevy_asset_packer::{asset_bundling_options::AssetBundlingOptions, bundled_asset_plugin::BundledAssetIoPlugin};

#[derive(Component)]
enum Direction {
    Up,
    Down,
}

fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.encryption_on = true;
    options.compress_on = true;
    options.set_encryption_key("very_secret_key".to_owned());

    App::new()
        .add_plugins(
            DefaultPlugins
            .set(LogPlugin {
                level: Level::INFO,
                ..Default::default()
            })
                .build()
                .add_before::<bevy::asset::AssetPlugin, _>(BundledAssetIoPlugin::from(options)),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, sprite_movement)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn((
        SpriteBundle {
            texture: asset_server.load("images/bevy_logo.png"),
            transform: Transform::from_xyz(100., 0., 0.),
            ..default()
        },
        Direction::Up,
    ));
}

fn sprite_movement(time: Res<Time>, mut sprite_position: Query<(&mut Direction, &mut Transform)>) {
    for (mut logo, mut transform) in &mut sprite_position {
        match *logo {
            Direction::Up => transform.translation.y += 150. * time.delta_seconds(),
            Direction::Down => transform.translation.y -= 150. * time.delta_seconds(),
        }

        if transform.translation.y > 200. {
            *logo = Direction::Down;
        } else if transform.translation.y < -200. {
            *logo = Direction::Up;
        }
    }
}
