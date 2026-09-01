//! The workspace's bootstrap; `cargo xtask --help` lists what it does.
use clap::{Parser, Subcommand};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::{env, fs};

const PATCHED: &str = ".patched";
const PATCHES: &str = "patches";
const BASE: &str = "base";
const APPLIED: &str = ".git/xtask-applied";

/// Bootstrap the patched cranelift checkout the workspace builds against.
///
/// `patches/` holds one directory per upstream: a `base` naming the repository and the
/// commit to sit on, and the mailbox patches to apply over it, in file name order. The
/// directory name is the checkout's name under `.patched/`. With no command, builds.
#[derive(Parser)]
#[command(bin_name = "cargo xtask", version)]
struct Cli {
    #[command(subcommand)]
    task: Option<Task>,
}

#[derive(Subcommand)]
enum Task {
    /// Patch every upstream that is not current, then build the workspace
    Build,
    /// Delete the patched checkouts, then clean the workspace
    Clean,
    /// Rebuild the checkouts from `patches/`, discarding what is in them
    Am {
        /// Directory under `patches/` to act on, or every one of them
        upstream: Option<String>,
    },
    /// Write the checkouts' commits back over the patches they came from
    FormatPatch {
        /// Directory under `patches/` to act on, or every one of them
        upstream: Option<String>,
    },
}

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_owned();

    match Cli::parse().task.unwrap_or(Task::Build) {
        Task::Build => {
            prepare(&root);
            cargo(&root, "build")
        }
        Task::Clean => {
            fs::remove_dir_all(root.join(PATCHED)).ok();
            cargo(&root, "clean")
        }
        Task::Am { upstream } => {
            for picked in select(&root, upstream.as_deref()) {
                reapply(&root, &picked);
            }
            ExitCode::SUCCESS
        }
        Task::FormatPatch { upstream } => {
            for picked in select(&root, upstream.as_deref()) {
                format_patch(&root, &picked);
            }
            ExitCode::SUCCESS
        }
    }
}

fn prepare(root: &Path) {
    for upstream in upstreams(root) {
        sync(root, &upstream);
    }
}

fn upstreams(root: &Path) -> Vec<Upstream> {
    let dir = root.join(PATCHES);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("a patches directory")
        .map(|entry| entry.expect("reading patches/").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "patches/ holds no upstream to patch");
    dirs.iter().map(|dir| upstream(dir)).collect()
}

fn select(root: &Path, which: Option<&str>) -> Vec<Upstream> {
    let all = upstreams(root);
    let Some(which) = which else {
        return all;
    };
    let picked: Vec<Upstream> = all
        .into_iter()
        .filter(|upstream| upstream.name == which)
        .collect();
    assert!(!picked.is_empty(), "patches/ holds no upstream `{which}`");
    picked
}

struct Upstream {
    name: String,
    repo: String,
    rev: String,
    series: Vec<PathBuf>,
}

fn upstream(dir: &Path) -> Upstream {
    let name = name(dir).to_owned();
    let (repo, rev) = base(&dir.join(BASE));
    let series = series(dir);
    Upstream {
        name,
        repo,
        rev,
        series,
    }
}

fn base(path: &Path) -> (String, String) {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let mut repo = None;
    let mut rev = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("in {}: `{line}` is not `key = value`", path.display()));
        let value = value.trim().to_owned();
        match key.trim() {
            "repo" => repo = Some(value),
            "rev" => rev = Some(value),
            other => panic!("in {}: `{other}` is not `repo` or `rev`", path.display()),
        }
    }
    let repo = repo.unwrap_or_else(|| panic!("{} sets no `repo`", path.display()));
    let rev = rev.unwrap_or_else(|| panic!("{} sets no `rev`", path.display()));
    assert!(
        rev.len() == 40 && rev.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "in {}: `rev` wants a full commit id, not `{rev}`",
        path.display()
    );
    (repo, rev)
}

fn series(dir: &Path) -> Vec<PathBuf> {
    let mut series: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.expect("reading a patch directory").path())
        .filter(|path| path.extension() == Some(OsStr::new("patch")))
        .collect();
    series.sort();
    assert!(
        !series.is_empty(),
        "{} holds no patch to apply",
        dir.display()
    );
    series
}

fn sync(root: &Path, upstream: &Upstream) {
    let applied = checkout(root, upstream).join(APPLIED);
    if fs::read_to_string(applied).is_ok_and(|current| current == stamp(root, upstream)) {
        return;
    }
    reapply(root, upstream);
}

fn reapply(root: &Path, upstream: &Upstream) {
    let checkout = checkout(root, upstream);
    fetch(&checkout, upstream);
    rewind(&checkout, &upstream.rev);
    apply(&checkout, upstream);
    fs::write(checkout.join(APPLIED), stamp(root, upstream))
        .expect("recording which series is applied");
}

fn checkout(root: &Path, upstream: &Upstream) -> PathBuf {
    root.join(PATCHED).join(&upstream.name)
}

fn stamp(root: &Path, upstream: &Upstream) -> String {
    let out = Command::new("git")
        .arg("hash-object")
        .arg("--")
        .args(&upstream.series)
        .current_dir(root)
        .output()
        .expect("hashing the series");
    assert!(out.status.success(), "git hash-object failed");
    let ids = String::from_utf8(out.stdout).expect("git prints object ids in hex");

    let mut stamp = format!("{} {}\n", upstream.repo, upstream.rev);
    for (patch, id) in upstream.series.iter().zip(ids.lines()) {
        stamp.push_str(&format!("{id} {}\n", name(patch)));
    }
    stamp
}

fn format_patch(root: &Path, upstream: &Upstream) {
    let checkout = checkout(root, upstream);
    let rev = upstream.rev.as_str();
    assert!(
        has_rev(&checkout, rev),
        "{}: nothing checked out at {rev}; `cargo xtask am` first",
        upstream.name
    );
    assert!(
        git_ok(&checkout, ["merge-base", "--is-ancestor", rev, "HEAD"]),
        "{}: HEAD has left {rev}, so `{rev}..HEAD` is not the series",
        upstream.name
    );

    let dir = root.join(PATCHES).join(&upstream.name);
    let out = Command::new("git")
        .args(["format-patch", "--zero-commit", "--no-signature", "-o"])
        .arg(&dir)
        .arg(format!("{rev}..HEAD"))
        .current_dir(&checkout)
        .output()
        .expect("running git format-patch");
    assert!(
        out.status.success(),
        "git format-patch failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );

    let listing = String::from_utf8(out.stdout).expect("git prints the paths it wrote");
    let fresh: Vec<&str> = listing.lines().map(|line| name(Path::new(line))).collect();
    assert!(
        !fresh.is_empty(),
        "{}: no commit on top of {rev} to export",
        upstream.name
    );
    for stale in series(&dir) {
        if !fresh.contains(&name(&stale)) {
            println!("xtask: {}: dropping {}", upstream.name, name(&stale));
            fs::remove_file(&stale).expect("removing a superseded patch");
        }
    }
    for patch in fresh {
        println!("xtask: {}: wrote {patch}", upstream.name);
    }
}

fn fetch(checkout: &Path, upstream: &Upstream) {
    let repo = upstream.repo.as_str();
    let rev = upstream.rev.as_str();
    if has_rev(checkout, rev) {
        return;
    }
    println!("xtask: fetching {repo} at {}", &rev[..12]);
    fs::create_dir_all(checkout).expect("somewhere to fetch into");
    if !checkout.join(".git").exists() {
        git(checkout, ["init", "-q"]);
    }
    // The url rather than a named remote: nothing to go stale when `base` moves.
    git(checkout, ["fetch", "-q", "--depth", "1", repo, rev]);
    git(checkout, ["checkout", "-q", rev]);
}

fn has_rev(checkout: &Path, rev: &str) -> bool {
    let commit = format!("{rev}^{{commit}}");
    git_ok(checkout, ["cat-file", "-e", commit.as_str()])
}

fn rewind(checkout: &Path, rev: &str) {
    if checkout.join(".git/rebase-apply").exists() {
        git(checkout, ["am", "--abort"]);
    }
    git(checkout, ["reset", "--hard", "-q", rev]);
}

fn apply(checkout: &Path, upstream: &Upstream) {
    const WHO: [&str; 4] = ["-c", "user.name=xtask", "-c", "user.email=xtask@invalid"];

    for patch in &upstream.series {
        println!("xtask: {}: applying {}", upstream.name, name(patch));
        let mut args: Vec<&OsStr> = WHO.iter().map(OsStr::new).collect();
        args.extend([OsStr::new("am"), OsStr::new("-q"), patch.as_os_str()]);
        git(checkout, args);
    }
}

fn name(path: &Path) -> &str {
    path.file_name()
        .and_then(OsStr::to_str)
        .expect("a file name that is utf-8")
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

fn git_ok<S: AsRef<OsStr>>(at: &Path, args: impl IntoIterator<Item = S>) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(at)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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
