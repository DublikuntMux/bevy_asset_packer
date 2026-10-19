use bevy::{asset::AssetPlugin, prelude::*};
use bevy_asset_packer::{AssetBundle, BundledAssetPlugin};

const HELLO: &[u8] = b"Hello from the bundle!\n";

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    // Reads only the header and index now; assets are read from disk on demand.
    let bundle = AssetBundle::from_file(concat!(env!("OUT_DIR"), "/assets.pack"), None)?;
    let hello = futures_lite::future::block_on(bundle.read("hello.txt"))?;
    assert_eq!(hello.as_deref(), Some(HELLO));
    run_app(bundle);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        // Publish the build script's assets.pack beside the web application.
        let bundle = AssetBundle::from_url("/assets.pack", None)
            .await
            .expect("open assets.pack using HTTP range requests");
        let hello = bundle.read("hello.txt").await.expect("read bundled asset");
        assert_eq!(hello.as_deref(), Some(HELLO));
        run_app(bundle);
    });
}

fn run_app(bundle: AssetBundle) {
    // The bundle plugin must be added before AssetPlugin/DefaultPlugins.
    let mut app = App::new();
    app.add_plugins((
        BundledAssetPlugin::new(bundle),
        MinimalPlugins,
        AssetPlugin::default(),
    ));
    app.update();
    println!("Packed assets are registered.");
}
