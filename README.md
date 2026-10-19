# Bevy Asset Packer

Pack Bevy assets into an indexed `.pack` archive and load individual files on demand, including in browser builds.

## Capabilities

- Preserve nested paths, `.meta` files, hidden files, and empty directories.
- Compress each file independently with zlib/DEFLATE (on by default), storing it uncompressed when compression does not save space.
- Optionally authenticate and encrypt the index (including filenames) and each file with AES-128-GCM.
- Read from an external file on native platforms, a separate URL in browsers, or embedded bytes on either platform.
- Load the header and index when opening a bundle; read and decode each asset only when requested.

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets");
    let output = PathBuf::from(env::var("OUT_DIR")?).join("assets.pack");
    AssetBundler::new("assets", output, AssetBundlingOptions::default()).build()?;
    Ok(())
}
```

## Native external pack

Register the bundle plugin **before `DefaultPlugins`**:

```rust
use bevy::prelude::*;
use bevy_asset_packer::{AssetBundle, BundledAssetPlugin};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = AssetBundle::from_file("assets.pack", None)?;
    App::new()
        .add_plugins((BundledAssetPlugin::new(bundle), DefaultPlugins))
        .run();
    Ok(())
}
```

Opening reads only the header and index. Asset reads seek to the requested file's stored bytes. Keep the pack available and unchanged for the lifetime of the bundle.

Assets are requested through `AssetServer` by the **same relative paths** they had under `assets/`. Bevy still needs the appropriate loader for each asset type.

## Browser pack with HTTP range requests

Add a browser task executor:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen-futures = "0.4"
```

Start the app after opening the remote index:

```rust
use bevy::prelude::*;
use bevy_asset_packer::{AssetBundle, BundledAssetPlugin};

fn main() {
    wasm_bindgen_futures::spawn_local(async {
        let bundle = AssetBundle::from_url("/assets-v2.pack", None)
            .await
            .expect("open asset bundle");
        App::new()
            .add_plugins((BundledAssetPlugin::new(bundle), DefaultPlugins))
            .run();
    });
}
```

The `.pack` remains separate from the WASM download. Opening fetches the header and index; each requested asset fetches only its byte range. `from_url` is available on `wasm32` browser targets; `from_file` is native only.

Configure your server or CDN to:

- Honor `Range` requests with **`206 Partial Content`** and a valid `Content-Range` response. Servers that return the whole pack with `200 OK` are rejected; there is no full-download fallback.
- Disable HTTP content encoding/compression for `.pack` responses so byte offsets refer to the stored pack.
- For a different origin, allow the app's origin with `Access-Control-Allow-Origin` and expose `Content-Range` with `Access-Control-Expose-Headers: Content-Range`.

Each read loads and decompresses a **complete requested asset**, rather than streaming chunks within one asset. Memory use therefore scales with requested assets and their concurrent reads; the index remains resident. Bevy manages loaded assets normally, and browser HTTP caching follows your server's headers.

## AES-GCM encryption

Build with a 16-byte key:

```rust
let options = AssetBundlingOptions {
    compress: true,
    encryption_key: Some(*b"sample-key-16byt"), // replace with your own key
};
AssetBundler::new("assets", output, options).build()?;
```

Pass the same key to `from_file`, `from_bytes`, or `from_url`. The index is authenticated when opening; each encrypted asset is authenticated before decoding. A wrong key fails at opening, while modified asset ciphertext fails when that asset is requested. Each build uses a fresh nonce, and files use distinct derived nonces.

A decryption key embedded in a game is accessible to somebody who can inspect the executable. **Do not use this mechanism to store secrets.**

For a working headless example, run:

```sh
cargo run -p bundled_asset_example
```

And its all!!!
