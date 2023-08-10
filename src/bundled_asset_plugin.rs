use bevy::{
    app::{App, Plugin},
    asset::AssetServer,
};

use crate::{asset_bundling_options::AssetBundlingOptions, bundled_asset_io::BundledAssetIo};

#[derive(Default)]
pub struct BundledAssetIoPlugin {
    options: AssetBundlingOptions,
}

impl From<AssetBundlingOptions> for BundledAssetIoPlugin {
    fn from(options: AssetBundlingOptions) -> Self {
        Self { options }
    }
}

impl Plugin for BundledAssetIoPlugin {
    fn build(&self, app: &mut App) {
        let mut io = BundledAssetIo::from(self.options.clone());
        match io.ensure_loaded() {
            _ => {
                app.insert_resource(AssetServer::new(io));
            }
        }
    }

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}
