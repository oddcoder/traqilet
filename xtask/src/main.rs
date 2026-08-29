//! `cargo xtask build`, and `cargo xtask clean`.
//!
//! Cranelift keeps the half of a backend that is not an instruction set -- register
//! allocation, branch fixups, the frame -- in a private module, so a target it does not
//! ship cannot be written outside its crate. `patches/` opens that module; this applies
//! it to a copy of the crate, which `[patch.crates-io]` points at.
//!
//! It cannot be a build script: cargo reads `[patch.crates-io]` before it compiles
//! anything, so the copy has to exist before the workspace can be read at all. That is
//! also why this is its own workspace rather than a member of that one.

use std::path::Path;
use std::process::{Command, ExitCode};
use std::{env, fs};

const REPO: &str = "https://github.com/bytecodealliance/wasmtime";
const REV: &str = "d8a0da6d661605713798c1c9c76be5c28e3159ff";
const OUT: &str = ".patched/wasmtime";
const CRATE: &str = ".patched/wasmtime/cranelift/codegen";
const PATCH: &str = "patches/machinst-pub.patch";

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_owned();

    match env::args().nth(1).as_deref().unwrap_or("build") {
        "build" => {
            patch(&root);
            cargo(&root, "build")
        }
        "clean" => {
            fs::remove_dir_all(root.join(".patched")).ok();
            cargo(&root, "clean")
        }
        other => {
            eprintln!("xtask: `build` or `clean`, not `{other}`");
            ExitCode::FAILURE
        }
    }
}

fn patch(root: &Path) {
    let out = root.join(OUT);
    if out.exists() {
        return;
    }
    println!("xtask: fetching cranelift at {}", &REV[..12]);
    fs::create_dir_all(&out).expect("somewhere to fetch into");
    run("git", &["init".as_ref(), "-q".as_ref()], &out);
    run(
        "git",
        &["remote".as_ref(), "add".as_ref(), "origin".as_ref(), REPO.as_ref()],
        &out,
    );
    run(
        "git",
        &["fetch".as_ref(), "-q".as_ref(), "--depth".as_ref(), "1".as_ref(), "origin".as_ref(), REV.as_ref()],
        &out,
    );
    run("git", &["checkout".as_ref(), "-q".as_ref(), "FETCH_HEAD".as_ref()], &out);
    run(
        "git",
        &[
            "apply".as_ref(),
            "--directory".as_ref(),
            CRATE.as_ref(),
            PATCH.as_ref(),
        ],
        root,
    );
}

fn cargo(root: &Path, task: &str) -> ExitCode {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let ok = Command::new(cargo)
        .arg(task)
        .current_dir(root)
        .status()
        .is_ok_and(|status| status.success());
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(program: &str, args: &[&std::ffi::OsStr], at: &Path) {
    let status = Command::new(program)
        .args(args)
        .current_dir(at)
        .status()
        .unwrap_or_else(|e| panic!("running {program}: {e}"));
    assert!(status.success(), "{program} failed");
}
