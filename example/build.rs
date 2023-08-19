use bevy_asset_packer::{asset_bundler::AssetBundler, asset_bundling_options::AssetBundlingOptions};

fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.compress_on = true;
    options.set_encryption_key([57, 206, 200, 7, 215, 17, 45, 219, 131, 171, 8, 214, 85, 12, 129, 176]);
    AssetBundler::from(options).build().unwrap();
}
