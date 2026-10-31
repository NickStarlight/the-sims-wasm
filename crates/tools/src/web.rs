use super::{run, workspace};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

const WEB_ASSETS: &[&str] = &[
    "asset-cache.js",
    "audio-output.js",
    "callbacks.js",
    "compiled-cache.js",
    "conditional-suspension.js",
    "dispatch.js",
    "exceptions.js",
    "execution.js",
    "file-source.js",
    "folder.js",
    "frame-output.js",
    "frame-renderer.js",
    "game.js",
    "guest-memory.js",
    "guest.js",
    "icon-180.png",
    "icon-192.png",
    "icon-512.png",
    "icon.svg",
    "index.html",
    "keyboard.js",
    "launcher-background.png",
    "launcher-folder.png",
    "launcher-info.png",
    "launcher-music.wav",
    "launcher-play.png",
    "linker.js",
    "machine.js",
    "main.js",
    "manifest.webmanifest",
    "memory-ops.js",
    "native-imports.js",
    "presentation.js",
    "pwa.js",
    "runtime-access.js",
    "runtime-worker.js",
    "scheduler.js",
    "storage.js",
    "style.css",
    "sw.js",
    "sync-imports.js",
    "threads.js",
    "worker.js",
];

const GENERATED_ASSETS: &[&str] = &[
    "compiler.js",
    "compiler.d.ts",
    "compiler_bg.wasm",
    "compiler_bg.wasm.d.ts",
    "compiler-build.json",
    "runtime.js",
    "runtime.d.ts",
    "runtime_bg.wasm",
    "runtime_bg.wasm.d.ts",
];

pub(super) fn build_web() -> Result<()> {
    let staging = tempfile::tempdir_in(workspace())?;
    for package in ["sims-runtime", "sims-compiler"] {
        build_bindings(package, staging.path())?;
    }
    write_compiler_build(staging.path())?;
    stage_web_assets(&workspace().join("web"), staging.path())?;
    prepare_offline(staging.path())?;
    publish(staging.path(), &workspace().join("dist"))
}

fn stage_web_assets(source: &Path, destination: &Path) -> Result<()> {
    // Select application assets explicitly. A stray PE or old game bundle in
    // the web source directory must never become part of a published build.
    for name in WEB_ASSETS {
        let input = source.join(name);
        if !fs::symlink_metadata(&input)?.file_type().is_file() {
            bail!("Browser asset must be a regular file: {}", input.display());
        }
        fs::copy(input, destination.join(name))?;
    }
    Ok(())
}

fn compiler_build_manifest(javascript: &[u8], wasm: &[u8]) -> Result<serde_json::Value> {
    let mut identity = Sha256::new();
    identity.update(javascript);
    identity.update(wasm);
    let profile: serde_json::Value = serde_json::from_str(sims_compiler::complete_profile_json())?;
    Ok(serde_json::json!({
        "version": 1,
        "compilerId": format!("{:x}", identity.finalize()),
        "wasmSha256": format!("{:x}", Sha256::digest(wasm)),
        // Cache hits validate local inputs without initializing compiler Wasm.
        "profile": {
            "sha256": profile["sha256"],
            "modules": profile["modules"],
            "requiredFiles": profile["requiredFiles"]
        }
    }))
}

fn write_compiler_build(directory: &Path) -> Result<()> {
    let manifest = compiler_build_manifest(
        &fs::read(directory.join("compiler.js"))?,
        &fs::read(directory.join("compiler_bg.wasm"))?,
    )?;
    fs::write(
        directory.join("compiler-build.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}

fn validate_assets(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("non-UTF-8 browser asset name")?;
        if !entry.file_type()?.is_file()
            || (!WEB_ASSETS.contains(&name) && !GENERATED_ASSETS.contains(&name))
        {
            bail!("Unexpected browser artifact: {name}");
        }
    }
    for name in WEB_ASSETS.iter().chain(GENERATED_ASSETS) {
        if !directory.join(name).is_file() {
            bail!("Missing browser artifact: {name}");
        }
    }
    Ok(())
}

fn prepare_offline(directory: &Path) -> Result<()> {
    validate_assets(directory)?;
    let mut names = fs::read_dir(directory)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    names.retain(|name| name != "sw.js");
    let template = fs::read_to_string(directory.join("sw.js"))?;
    let mut hash = DefaultHasher::new();
    template.hash(&mut hash);
    for name in &names {
        name.hash(&mut hash);
        fs::read(directory.join(name))?.hash(&mut hash);
    }
    let worker = template
        .replace(
            "const precache = []",
            &format!("const precache = {}", serde_json::to_string(&names)?),
        )
        .replace("__BUILD_ID__", &format!("{:016x}", hash.finish()));
    fs::write(directory.join("sw.js"), worker)?;
    Ok(())
}

// Keep the previous complete build until every artifact has been generated.
fn publish(staging: &Path, destination: &Path) -> Result<()> {
    validate_assets(staging)?;
    let backup = tempfile::tempdir_in(destination.parent().context("missing output parent")?)?;
    let previous = backup.path().join("previous");
    let exists = destination.try_exists()?;
    if exists {
        fs::rename(destination, &previous)?;
    }
    if let Err(error) = fs::rename(staging, destination) {
        if exists {
            fs::rename(&previous, destination).context("restoring previous browser build")?;
        }
        return Err(error).context("publishing browser build");
    }
    Ok(())
}

fn build_bindings(package: &str, destination: &Path) -> Result<()> {
    run(
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--features",
            "browser",
        ],
    )?;
    let metadata = Command::new("cargo")
        .current_dir(workspace())
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()?;
    if !metadata.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&metadata.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout)?;
    let target = metadata["target_directory"]
        .as_str()
        .context("missing Cargo target directory")?;
    let filename = format!("{}.wasm", package.replace('-', "_"));
    let artifact = PathBuf::from(target)
        .join("wasm32-unknown-unknown/release")
        .join(filename);
    let output = package
        .strip_prefix("sims-")
        .context("invalid browser package name")?;
    run(
        "wasm-bindgen",
        &[
            "--target",
            "web",
            "--out-dir",
            destination.to_str().context("non-UTF-8 output path")?,
            "--out-name",
            output,
            artifact.to_str().context("non-UTF-8 artifact path")?,
        ],
    )
    .with_context(|| format!("generating {package} browser bindings"))
}

pub(super) fn serve(port: u16) -> Result<()> {
    build_web()?;
    let root = workspace().join("dist").canonicalize()?;
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|error| anyhow::anyhow!("cannot start server: {error}"))?;
    println!("Open http://localhost:{port}");
    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/");
        let path = if path == "/" {
            "index.html"
        } else {
            path.trim_start_matches('/')
        };
        let file = root.join(path).canonicalize();
        let response = match file {
            Ok(file) if file.starts_with(&root) && file.is_file() => {
                let mime = match file.extension().and_then(|value| value.to_str()) {
                    Some("js") => "text/javascript",
                    Some("css") => "text/css",
                    Some("wasm") => "application/wasm",
                    Some("html") => "text/html; charset=utf-8",
                    Some("json") => "application/json",
                    Some("webmanifest") => "application/manifest+json",
                    Some("svg") => "image/svg+xml",
                    Some("png") => "image/png",
                    _ => "application/octet-stream",
                };
                let header = tiny_http::Header::from_bytes("Content-Type", mime)
                    .map_err(|_| anyhow::anyhow!("invalid content type"))?;
                // Bodies are already buffered; send their known size even for
                // large Wasm downloads instead of adding chunked framing.
                tiny_http::Response::from_data(fs::read(file)?)
                    .with_header(header)
                    .with_chunked_threshold(usize::MAX)
            }
            _ => tiny_http::Response::from_data(b"Not found".to_vec()).with_status_code(404),
        };
        if let Err(error) = request.respond(response) {
            eprintln!("response failed: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/web.rs"]
mod tests;
