use std::sync::Arc;

use bevy::{
    app::{App, Plugin},
    asset::{
        AssetApp,
        io::{AssetSourceBuilder, AssetSourceId},
    },
};

use crate::{AssetBundle, BundledAssetReader};

/// Replaces the *default* asset source with an immutable in-memory bundle.
///
/// Add this plugin before `AssetPlugin` or `DefaultPlugins`. Bevy asset sources
/// are finalized while `AssetPlugin` initializes, so registering later is too late.
#[derive(Clone)]
pub struct BundledAssetPlugin {
    bundle: Arc<AssetBundle>,
}

impl BundledAssetPlugin {
    #[must_use]
    pub fn new(bundle: AssetBundle) -> Self {
        Self {
            bundle: Arc::new(bundle),
        }
    }
}

impl Plugin for BundledAssetPlugin {
    fn build(&self, app: &mut App) {
        let bundle = Arc::clone(&self.bundle);
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(BundledAssetReader::new(Arc::clone(&bundle)))),
        );
    }
}
