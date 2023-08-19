# Bevy asset packer

Easy to use bevy plugin for packing resources in single file and protect him.

## Features

- [x] Paking all assets in single file.
- [x] Encrypt assets.
- [x] Compressing assets.
- [ ] Load from externel bundle.

## Usage

### Dependency

Add to `Cargo.toml`:

```toml
[build-dependencies]
bevy_asset_packer = "0.3.0"

[dependencies]
bevy_asset_packer = "0.3.0"
```

### System setup

In src/main.rs

```rust
fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.compress_on = true;
    options.set_encryption_key([57, 206, 200, 7, 215, 17, 45, 219, 131, 171, 8, 214, 85, 12, 129, 176]);

    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .add_before::<bevy::asset::AssetPlugin, _>(BundledAssetIoPlugin::from(options)),
        )
        .run();
}
```

In build.rs

```rust
fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.compress_on = true;
    options.set_encryption_key([57, 206, 200, 7, 215, 17, 45, 219, 131, 171, 8, 214, 85, 12, 129, 176]);
    AssetBundler::from(options).build().unwrap();
}
```

You can see examle in example folder.  
And its all!!!
