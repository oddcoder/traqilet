//! `cargo xtask build`, and `cargo xtask clean`.
//!
//! Cranelift keeps the half of a backend that is not an instruction set -- register
//! allocation, branch fixups, the frame -- in a private module, so a target it does not
//! ship cannot be written outside its crate. `patches/` opens that module: a series of
//! mailbox patches, applied with `git am` in file name order on top of a pinned wasmtime
//! revision, which the workspace then depends on by path.
//!
//! It cannot be a build script: cargo resolves that path dependency before it compiles
//! anything, so the checkout has to exist before the workspace can be read at all. That is
//! also why this is its own workspace rather than a member of that one.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::{env, fs};

const REPO: &str = "https://github.com/bytecodealliance/wasmtime";
const REV: &str = "d8a0da6d661605713798c1c9c76be5c28e3159ff";
const PATCHED: &str = ".patched";
const CHECKOUT: &str = ".patched/wasmtime";
const APPLIED: &str = ".patched/applied";
const PATCHES: &str = "patches";

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_owned();

    match env::args().nth(1).as_deref().unwrap_or("build") {
        "build" => {
            prepare(&root);
            cargo(&root, "build")
        }
        "clean" => {
            fs::remove_dir_all(root.join(PATCHED)).ok();
            cargo(&root, "clean")
        }
        other => {
            eprintln!("xtask: `build` or `clean`, not `{other}`");
            ExitCode::FAILURE
        }
    }
}

fn prepare(root: &Path) {
    let series = series(root);
    let stamp = stamp(root, &series);
    let applied = root.join(APPLIED);
    if fs::read_to_string(&applied).is_ok_and(|current| current == stamp) {
        return;
    }
    let checkout = root.join(CHECKOUT);
    fetch(&checkout);
    rewind(&checkout);
    apply(&checkout, &series);
    fs::write(&applied, stamp).expect("recording which series is applied");
}

fn series(root: &Path) -> Vec<PathBuf> {
    let dir = root.join(PATCHES);
    let mut series: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("a patches directory")
        .map(|entry| entry.expect("reading patches/").path())
        .filter(|path| path.extension() == Some(OsStr::new("patch")))
        .collect();
    series.sort();
    assert!(!series.is_empty(), "patches/ holds no patch to apply");
    series
}

fn stamp(root: &Path, series: &[PathBuf]) -> String {
    let out = Command::new("git")
        .arg("hash-object")
        .arg("--")
        .args(series)
        .current_dir(root)
        .output()
        .expect("hashing the series");
    assert!(out.status.success(), "git hash-object failed");
    let ids = String::from_utf8(out.stdout).expect("git prints object ids in hex");

    let mut stamp = format!("{REV}\n");
    for (patch, id) in series.iter().zip(ids.lines()) {
        stamp.push_str(&format!("{id} {}\n", name(patch)));
    }
    stamp
}

fn fetch(checkout: &Path) {
    if has_rev(checkout) {
        return;
    }
    println!("xtask: fetching cranelift at {}", &REV[..12]);
    fs::create_dir_all(checkout).expect("somewhere to fetch into");
    if !checkout.join(".git").exists() {
        git(checkout, ["init", "-q"]);
        git(checkout, ["remote", "add", "origin", REPO]);
    }
    git(checkout, ["fetch", "-q", "--depth", "1", "origin", REV]);
    git(checkout, ["checkout", "-q", REV]);
}

fn has_rev(checkout: &Path) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{REV}^{{commit}}")])
        .current_dir(checkout)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn rewind(checkout: &Path) {
    // `git am` refuses to start while a series that failed halfway is still in progress.
    if checkout.join(".git/rebase-apply").exists() {
        git(checkout, ["am", "--abort"]);
    }
    git(checkout, ["reset", "--hard", "-q", REV]);
}

fn apply(checkout: &Path, series: &[PathBuf]) {
    // The commits are scratch -- nobody reads this history -- but `am` still wants a
    // committer, and the patches only name their author.
    const WHO: [&str; 4] = ["-c", "user.name=xtask", "-c", "user.email=xtask@invalid"];

    let mut args: Vec<&OsStr> = WHO.iter().map(OsStr::new).collect();
    args.push(OsStr::new("am"));
    args.extend(series.iter().map(|patch| patch.as_os_str()));
    git(checkout, args);
}

fn name(patch: &Path) -> &str {
    patch
        .file_name()
        .and_then(OsStr::to_str)
        .expect("a patch file name that is utf-8")
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

fn git<S: AsRef<OsStr>>(at: &Path, args: impl IntoIterator<Item = S>) {
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();
    let status = Command::new("git")
        .args(&args)
        .current_dir(at)
        .status()
        .unwrap_or_else(|e| panic!("running git: {e}"));
    assert!(
        status.success(),
        "in {}: git {} failed",
        at.display(),
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
}
