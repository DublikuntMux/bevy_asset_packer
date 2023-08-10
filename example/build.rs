use bevy_asset_packer::{asset_bundling_options::AssetBundlingOptions, asset_bundler::AssetBundler};

fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.encryption_on = true;
    options.compress_on = true;
    options.set_encryption_key("very_secret_key".to_owned());
    AssetBundler::from(options).build().unwrap();
}
