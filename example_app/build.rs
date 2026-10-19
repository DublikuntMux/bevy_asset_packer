use bevy_asset_packer::{AssetBundler, AssetBundlingOptions};
use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=assets");
    let path = PathBuf::from(env::var("OUT_DIR")?).join("assets.pack");
    AssetBundler::new("assets", path, AssetBundlingOptions::default()).build()
}
