// Browser futures (`fetch`, `JsFuture`) are single-threaded and cannot be `Send`;
// Bevy's asset reader API drops the `Send` bound on wasm for the same reason.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::future_not_send,
        reason = "browser futures cannot be Send on wasm"
    )
)]

mod bundle;
#[cfg(not(target_arch = "wasm32"))]
mod bundler;
mod format;
mod options;
#[cfg(feature = "runtime")]
mod plugin;
#[cfg(feature = "runtime")]
mod reader;
mod source;

pub use bundle::AssetBundle;
#[cfg(not(target_arch = "wasm32"))]
pub use bundler::AssetBundler;
pub use options::AssetBundlingOptions;
#[cfg(feature = "runtime")]
pub use plugin::BundledAssetPlugin;
#[cfg(feature = "runtime")]
pub use reader::BundledAssetReader;
