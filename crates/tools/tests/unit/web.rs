use super::*;

const WORKER: &str = "const precache = []; const version = '__BUILD_ID__';";

fn application(directory: &Path) -> Result<()> {
    for name in WEB_ASSETS.iter().chain(GENERATED_ASSETS) {
        fs::write(directory.join(name), name.as_bytes())?;
    }
    fs::write(directory.join("sw.js"), WORKER)?;
    write_compiler_build(directory)
}

#[test]
fn staging_ignores_binaries_and_old_prepared_artifacts() -> Result<()> {
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    application(source.path())?;
    for name in [
        "Sims.exe",
        "GIMEX.DLL",
        "game.ICD",
        "client.wasm",
        "client.json",
        "client-memory.bin",
        "other.js",
    ] {
        fs::write(source.path().join(name), "private game bytes")?;
    }
    stage_web_assets(source.path(), destination.path())?;
    assert_eq!(fs::read_dir(destination.path())?.count(), WEB_ASSETS.len());
    for name in WEB_ASSETS {
        assert_eq!(
            fs::read(source.path().join(name))?,
            fs::read(destination.path().join(name))?
        );
    }
    assert!(!destination.path().join("client.wasm").exists());
    assert!(!destination.path().join("Sims.exe").exists());
    assert!(
        !destination.path().join("compiler-build.json").exists(),
        "generated files cannot be overridden by web sources"
    );
    Ok(())
}

#[test]
fn publication_and_precache_reject_unlisted_or_incomplete_artifacts() -> Result<()> {
    for name in [
        "Sims.exe",
        "GIMEX.DLL",
        "game.ICD",
        "client.wasm",
        "client.json",
        "client-memory.bin",
        "private.json",
    ] {
        let staging = tempfile::tempdir()?;
        application(staging.path())?;
        fs::write(staging.path().join(name), "private game bytes")?;
        assert!(prepare_offline(staging.path()).is_err(), "{name}");
        assert_eq!(fs::read_to_string(staging.path().join("sw.js"))?, WORKER);
        let root = tempfile::tempdir()?;
        let destination = root.path().join("dist");
        fs::create_dir(&destination)?;
        fs::write(destination.join("previous"), "keep")?;
        assert!(publish(staging.path(), &destination).is_err(), "{name}");
        assert_eq!(fs::read_to_string(destination.join("previous"))?, "keep");
    }
    for name in GENERATED_ASSETS {
        let staging = tempfile::tempdir()?;
        application(staging.path())?;
        fs::remove_file(staging.path().join(name))?;
        assert!(prepare_offline(staging.path()).is_err(), "{name}");
    }
    let staging = tempfile::tempdir()?;
    application(staging.path())?;
    fs::create_dir(staging.path().join("nested"))?;
    assert!(validate_assets(staging.path()).is_err());
    Ok(())
}

#[test]
fn compiler_identity_tracks_glue_and_wasm_and_exposes_only_cache_profile_data() -> Result<()> {
    let first = compiler_build_manifest(b"ab", b"c")?;
    assert_eq!(first["version"], 1);
    assert_eq!(
        first["compilerId"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        first["wasmSha256"],
        "2e7d2c03a9507ae265ecf5b5356885a53393a2029d241394997265a1a25aefc6"
    );
    assert_eq!(first, compiler_build_manifest(b"ab", b"c")?);
    let glue = compiler_build_manifest(b"changed glue", b"c")?;
    assert_ne!(first["compilerId"], glue["compilerId"]);
    assert_eq!(first["wasmSha256"], glue["wasmSha256"]);
    let wasm = compiler_build_manifest(b"ab", b"changed wasm")?;
    assert_ne!(first["compilerId"], wasm["compilerId"]);
    assert_ne!(first["wasmSha256"], wasm["wasmSha256"]);
    let profile: serde_json::Value = serde_json::from_str(sims_compiler::complete_profile_json())?;
    assert_eq!(first["profile"].as_object().unwrap().len(), 3);
    for key in ["sha256", "modules", "requiredFiles"] {
        assert_eq!(first["profile"][key], profile[key]);
    }
    assert!(first["profile"].get("roots").is_none());
    assert!(first["profile"].get("initialRegistry").is_none());
    Ok(())
}

#[test]
fn offline_manifest_contains_compiler_and_invalidates_for_compiler_changes() -> Result<()> {
    let root = tempfile::tempdir()?;
    application(root.path())?;
    prepare_offline(root.path())?;
    let first = fs::read_to_string(root.path().join("sw.js"))?;
    for name in [
        "compiler.js",
        "compiler_bg.wasm",
        "compiler-build.json",
        "runtime_bg.wasm",
        "index.html",
    ] {
        assert!(first.contains(&format!("\"{name}\"")), "{name}");
    }
    assert!(!first.contains("client.wasm"));
    assert!(!first.contains("__BUILD_ID__"));
    fs::write(root.path().join("sw.js"), WORKER)?;
    prepare_offline(root.path())?;
    assert_eq!(first, fs::read_to_string(root.path().join("sw.js"))?);
    let manifest = fs::read(root.path().join("compiler-build.json"))?;
    for name in ["compiler.js", "compiler_bg.wasm"] {
        fs::write(root.path().join("sw.js"), WORKER)?;
        fs::write(root.path().join(name), "new compiler version")?;
        write_compiler_build(root.path())?;
        prepare_offline(root.path())?;
        assert_ne!(first, fs::read_to_string(root.path().join("sw.js"))?);
        assert_ne!(manifest, fs::read(root.path().join("compiler-build.json"))?);
    }
    Ok(())
}

#[test]
fn publication_replaces_old_game_payloads_instead_of_merging() -> Result<()> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("dist");
    fs::create_dir(&destination)?;
    fs::write(destination.join("client.wasm"), b"old game")?;
    let staging = tempfile::tempdir_in(root.path())?;
    application(staging.path())?;
    prepare_offline(staging.path())?;
    publish(staging.path(), &destination)?;
    validate_assets(&destination)?;
    assert!(!destination.join("client.wasm").exists());
    assert!(destination.join("compiler_bg.wasm").is_file());
    Ok(())
}

#[test]
fn failed_publication_preserves_previous_build() -> Result<()> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("dist");
    fs::create_dir(&destination)?;
    fs::write(destination.join("runtime.wasm"), b"previous")?;
    assert!(publish(&root.path().join("missing"), &destination).is_err());
    assert_eq!(fs::read(destination.join("runtime.wasm"))?, b"previous");
    Ok(())
}
