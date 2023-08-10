# Bevy asset packer

Easy to use bevy plugin for packing resources in single file and protect him.

## Features

- [x] Paking all assets in single file.
- [x] Encrypt assets.
- [ ] Compressing assets.

## Usage

### Dependency

Add to `Cargo.toml`:

```toml
[dependencies]
bevy_asset_packer = "0.1.0"
```

### System setup

In src/main.rs

```rust
fn main() {
    let mut options = AssetBundlingOptions::default();
    options.encode_file_names = true;
    options.encryption_on = true;
    options.set_encryption_key("very_secret_key".to_owned());

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
    options.encryption_on = true;
    options.set_encryption_key("very_secret_key".to_owned());
    AssetBundler::from(options).build().unwrap();
}
```

And its all!!!
