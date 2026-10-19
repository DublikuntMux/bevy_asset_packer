# Bevy Asset Packer

## Capabilities

- Pack nested assets (including `.meta` files and empty directories) into a single `.pack` file.
- Optionally compress the payload using zlib/DEFLATE (on by default).
- Optionally encrypt the complete archive, *including file paths*, using authenticated AES-128-GCM with a fresh nonce on every build.
- Embed the bundle in your executable at compile time, or load it externally.

## Requirements

- Bevy **0.20.0**
- Rust **1.97.1 or newer**

## Add to your game

```toml
[dependencies]
bevy = "0.20"
bevy_asset_packer = "0.5.0"

[build-dependencies]
bevy_asset_packer = { version = "0.5.0", default-features = false }
```

Add this build script at `build.rs`:

```rust
use std::{env, path::PathBuf};
use bevy_asset_packer::{AssetBundler, AssetBundlingOptions};

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=assets");
    let output = PathBuf::from(env::var("OUT_DIR")?).join("assets.pack");
    AssetBundler::new("assets", output, AssetBundlingOptions::default()).build()
}
```

The example uses `anyhow` in both the game and build script; alternatively use `Result<(), Box<dyn std::error::Error>>` for return type to avoid that dependency.

In `src/main.rs`, **register the bundle plugin before `DefaultPlugins`**:

```rust
use bevy::prelude::*;
use bevy_asset_packer::{AssetBundle, BundledAssetPlugin};

fn main() -> anyhow::Result<()> {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets.pack"));
    let bundle = AssetBundle::from_bytes(bytes, None)?;
    App::new()
        .add_plugins((BundledAssetPlugin::new(bundle), DefaultPlugins))
        .run();
    Ok(())
}
```

Assets can then be requested through `AssetServer` by the **same relative paths** they had under `assets/`. You still need Bevy's appropriate loader for the asset type.

## AES-GCM encryption

To build with a key:

```rust
let options = AssetBundlingOptions {
    compress: true,
    encryption_key: Some(*b"sample-key-16byt"), // replace with your own 16-byte key
};
AssetBundler::new("assets", output, options).build()?;
```

At runtime pass the **same key**:

```rust
let bundle = AssetBundle::from_bytes(bytes, Some(*b"sample-key-16byt"))?;
```

The file's header, content and filenames are authenticated. Incorrect keys or modified ciphertext fail to load. **Embedding a decryption key in the executable does not keep assets confidential from somebody who can inspect the executable.** Do not use this mechanism for storing secrets.

## Load an external pack instead

```rust
let bundle = AssetBundle::from_file("assets.pack", None)?;
app.add_plugins((BundledAssetPlugin::new(bundle), DefaultPlugins));
```

For a working headless example, run:

```sh
cargo run -p bundled_asset_example
```

And its all!!!
