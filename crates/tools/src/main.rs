mod web;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sims_compiler::Compiler;
use std::{fs, path::PathBuf, process::Command};

#[derive(Parser)]
#[command(about = "Build, check, and run the browser runtime")]
struct Arguments {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Enforce formatting, strict linting, and workspace tests.
    Check {
        /// Also package and exercise runtime Wasm and the local loader in Node.
        #[arg(long)]
        browser: bool,
    },
    /// Package the compiler, runtime, and browser host without game binaries.
    BuildWeb,
    /// Build and serve the browser application on localhost.
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Translate an executable into Wasm and linking metadata.
    Compile {
        executable: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Additional callback addresses, decimal or 0x-prefixed.
        #[arg(long, value_parser = address)]
        root: Vec<u32>,
        /// Additional PE image mapped at BASE=PATH (BASE accepts 0x).
        #[arg(long, value_parser = module)]
        module: Vec<Module>,
    },
}

fn main() -> Result<()> {
    match Arguments::parse().command {
        Task::Check { browser } => {
            check()?;
            if browser {
                web::build_web()?;
                run(
                    "node",
                    &["crates/runtime/tests/browser/main.mjs", "dist/runtime.js"],
                )?;
                run(
                    "node",
                    &[
                        "crates/tools/tests/browser/local-compiler.mjs",
                        "dist/game.js",
                    ],
                )?;
                run("node", &["crates/tools/tests/browser/compiled-cache.mjs"])?;
            }
            Ok(())
        }
        Task::BuildWeb => web::build_web(),
        Task::Serve { port } => web::serve(port),
        Task::Compile {
            executable,
            output,
            root,
            module,
        } => compile(executable, output, root, module),
    }
}

#[derive(Clone)]
struct Module {
    base: u32,
    path: PathBuf,
}

fn module(value: &str) -> Result<Module, String> {
    let (base, path) = value.split_once('=').ok_or("expected BASE=PATH")?;
    if path.is_empty() {
        return Err("module path is empty".into());
    }
    Ok(Module {
        base: address(base)?,
        path: path.into(),
    })
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn address(value: &str) -> Result<u32, String> {
    let parsed = match value.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => value.parse(),
    };
    parsed.map_err(|error| error.to_string())
}

fn run(program: &str, arguments: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .current_dir(workspace())
        .args(arguments)
        .status()
        .with_context(|| format!("could not start {program}"))?;
    if !status.success() {
        bail!("{program} {} failed: {status}", arguments.join(" "));
    }
    Ok(())
}

fn check() -> Result<()> {
    run("cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        "cargo",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    for package in ["sims-compiler", "sims-runtime"] {
        run(
            "cargo",
            &[
                "clippy",
                "--locked",
                "-p",
                package,
                "--target",
                "wasm32-unknown-unknown",
                "--features",
                "browser",
                "--",
                "-D",
                "warnings",
            ],
        )?;
    }
    run(
        "cargo",
        &["test", "--locked", "--workspace", "--all-features"],
    )
}

fn compile(
    executable: PathBuf,
    output: PathBuf,
    roots: Vec<u32>,
    modules: Vec<Module>,
) -> Result<()> {
    if output.with_extension("json") == output {
        bail!("output must use a different extension from the metadata .json file");
    }
    let bytes = fs::read(&executable).with_context(|| format!("read {}", executable.display()))?;
    let mut compiler = Compiler::new(&bytes)?;
    for module in modules {
        let bytes =
            fs::read(&module.path).with_context(|| format!("read {}", module.path.display()))?;
        compiler.add_module(&bytes, module.base)?;
    }
    let artifact = compiler.compile(&roots)?;
    if !artifact.metadata.diagnostics.is_empty() {
        eprintln!(
            "{} discovery diagnostics recorded in metadata",
            artifact.metadata.diagnostics.len()
        );
    }
    fs::write(output.with_extension("json"), artifact.metadata_json()?)?;
    fs::write(&output, &artifact.wasm)?;
    println!(
        "{} functions, {} bytes → {}",
        artifact.metadata.func_count,
        artifact.wasm.len(),
        output.display()
    );
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/cli.rs"]
mod tests;
