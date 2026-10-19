use std::fs;

#[cfg(feature = "runtime")]
use std::path::Path;

use bevy_asset_packer::{AssetBundle, AssetBundler, AssetBundlingOptions};

fn sample(options: AssetBundlingOptions) -> (tempfile::TempDir, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    fs::create_dir_all(assets.join("nested/empty")).unwrap();
    fs::write(assets.join("nested/hello.txt"), b"hello world").unwrap();
    fs::write(assets.join("nested/hello.txt.meta"), b"(meta: value)").unwrap();
    fs::write(assets.join(".hidden"), b"hidden").unwrap();
    let pack = AssetBundler::new(assets, root.path().join("output/assets.pack"), options);
    pack.build().unwrap();
    let bytes = fs::read(&pack.output_file).unwrap();
    (root, bytes)
}

#[test]
fn round_trip_plain_and_compressed() {
    for compress in [false, true] {
        let (_, bytes) = sample(AssetBundlingOptions {
            compress,
            encryption_key: None,
        });
        assert_eq!(&bytes[..5], b"BPACK");
        let bundle = AssetBundle::from_bytes(&bytes, None).unwrap();
        assert_eq!(bundle.get("nested/hello.txt"), Some(&b"hello world"[..]));
        assert_eq!(
            bundle.get("nested/hello.txt.meta"),
            Some(&b"(meta: value)"[..])
        );
        assert_eq!(bundle.get(".hidden"), Some(&b"hidden"[..]));
        assert!(bundle.get("missing.txt").is_none());
        assert!(bundle.get("../nested/hello.txt").is_none());
    }
}

#[test]
fn encrypted_round_trip_rejects_bad_key_and_tampering() {
    let key = *b"sample-key-16byt";
    let (_, bytes) = sample(AssetBundlingOptions {
        compress: true,
        encryption_key: Some(key),
    });
    let bundle = AssetBundle::from_bytes(&bytes, Some(key)).unwrap();
    assert_eq!(bundle.get("nested/hello.txt"), Some(&b"hello world"[..]));
    assert!(AssetBundle::from_bytes(&bytes, Some([42; 16])).is_err());
    assert!(AssetBundle::from_bytes(&bytes, None).is_err());
    let mut modified = bytes;
    *modified.last_mut().unwrap() ^= 0x01;
    assert!(AssetBundle::from_bytes(&modified, Some(key)).is_err());
}

#[test]
fn invalid_headers_are_rejected() {
    let (_, bytes) = sample(AssetBundlingOptions::default());
    assert!(AssetBundle::from_bytes(&bytes[..5], None).is_err());
    let mut invalid_magic = bytes.clone();
    invalid_magic[0] = 0;
    assert!(AssetBundle::from_bytes(&invalid_magic, None).is_err());
    let mut invalid_flags = bytes;
    invalid_flags[6] = 0xff;
    assert!(AssetBundle::from_bytes(&invalid_flags, None).is_err());
}

#[cfg(feature = "runtime")]
#[test]
fn bevy_reader_reads_assets_meta_and_directories() {
    use bevy::asset::io::{AssetReader, Reader as _};
    use bevy_asset_packer::BundledAssetReader;
    use futures_lite::{StreamExt, future::block_on};
    use std::sync::Arc;

    let (_, bytes) = sample(AssetBundlingOptions::default());
    let reader = BundledAssetReader::new(Arc::new(AssetBundle::from_bytes(&bytes, None).unwrap()));
    block_on(async {
        let mut stream = reader.read(Path::new("nested/hello.txt")).await.unwrap();
        let mut actual = Vec::new();
        stream.read_to_end(&mut actual).await.unwrap();
        assert_eq!(&actual, b"hello world");

        let mut metadata = reader
            .read_meta(Path::new("nested/hello.txt"))
            .await
            .unwrap();
        actual.clear();
        metadata.read_to_end(&mut actual).await.unwrap();
        assert_eq!(&actual, b"(meta: value)");

        assert!(
            reader
                .is_directory(Path::new("nested/empty"))
                .await
                .unwrap()
        );
        assert!(
            !reader
                .is_directory(Path::new("nested/hello.txt"))
                .await
                .unwrap()
        );
        let mut listing = reader.read_directory(Path::new("nested")).await.unwrap();
        let mut names = Vec::new();
        while let Some(path) = listing.next().await {
            names.push(path);
        }
        assert!(names.contains(&Path::new("nested/hello.txt").to_path_buf()));
        assert!(names.contains(&Path::new("nested/empty").to_path_buf()));
        assert!(!names.contains(&Path::new("nested/hello.txt.meta").to_path_buf()));
    });
}
