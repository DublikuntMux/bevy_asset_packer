use bevy::{asset::AssetPlugin, prelude::*};
use bevy_asset_packer::{AssetBundle, BundledAssetPlugin};

fn main() -> anyhow::Result<()> {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets.pack"));
    let bundle = AssetBundle::from_bytes(bytes, None)?;
    assert_eq!(
        bundle.get("hello.txt"),
        Some(&b"Hello from the bundle!\n"[..])
    );

    // The bundle plugin must be added before AssetPlugin/DefaultPlugins.
    let mut app = App::new();
    app.add_plugins((
        BundledAssetPlugin::new(bundle),
        MinimalPlugins,
        AssetPlugin::default(),
    ));
    app.update();
    println!("Packed assets are registered.");
    Ok(())
}
