//! Build script for the out-of-tree eBPF Cranelift backend.
//!
//! Cranelift's in-tree backends get their ISLE compiled by
//! `cranelift/codegen/build.rs`, driven by the fixed `Isa` enum and
//! `meta::isle::get_isle_compilations()`. Neither is extensible from outside
//! the crate, so we reproduce the two steps that matter here:
//!
//! 1. Ask `cranelift-codegen-meta` to emit the target-independent ISLE
//!    vocabulary: `clif_lower.isle` (an ISLE term per CLIF instruction) and
//!    `numerics.isle`. These are the terms our `lower.isle` rules match
//!    against, and nothing about them is target-specific — `generate_isle`
//!    takes no ISA list at all.
//! 2. Feed those, Cranelift's hand-written preludes, and our own isle files
//!    to the ISLE compiler as a single compilation unit, producing
//!    `generated_code.rs`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn env(var: &str) -> String {
    env::var(var).unwrap_or_else(|_| panic!("`{var}` must be set"))
}

fn profile_dir(out_dir: &Path) -> PathBuf {
    out_dir
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .unwrap_or(out_dir)
        .to_path_buf()
}

fn workspace_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CARGO_WORKSPACE_DIR") {
        let dir = PathBuf::from(dir);
        if dir.join("Cargo.toml").is_file() {
            return dir;
        }
    }
    Path::new(&env("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is not nested under a workspace root")
        .to_path_buf()
}

fn main() {
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env("OUT_DIR"));

    let codegen_src = workspace_dir()
        .join(".patched/wasmtime/cranelift/codegen/src")
        .canonicalize()
        .expect("cannot find the patched cranelift-codegen sources");

    let isle_out = profile_dir(&out_dir).join("isle-out");
    fs::create_dir_all(&isle_out).expect("could not create the ISLE source directory");

    if let Err(err) = cranelift_codegen_meta::generate_isle(&isle_out) {
        panic!("failed to generate ISLE from cranelift-codegen-meta: {err}");
    }

    let tracked = [
        codegen_src.join("prelude.isle"),
        codegen_src.join("prelude_lower.isle"),
        manifest_dir.join("src/inst.isle"),
        manifest_dir.join("src/lower.isle"),
    ];
    let untracked = [
        isle_out.join("numerics.isle"),
        isle_out.join("clif_lower.isle"),
    ];

    println!("cargo:rerun-if-changed=build.rs");
    for input in &tracked {
        println!("cargo:rerun-if-changed={}", input.display());
    }

    let inputs: Vec<&Path> = tracked
        .iter()
        .chain(untracked.iter())
        .map(PathBuf::as_path)
        .collect();

    let options = cranelift_isle::codegen::CodegenOptions {
        // We `include!` the generated file, and inner attributes are not
        // allowed there, so the `#![allow(..)]` pragmas live at the include
        // site instead.
        exclude_global_allow_pragmas: true,
        prefixes: [
            (&out_dir, "<OUT_DIR>"),
            (&isle_out, "<ISLE_DIR>"),
            (&codegen_src, "<CRANELIFT_SRC>"),
            (&manifest_dir, "<CRATE_DIR>"),
        ]
        .into_iter()
        .map(|(prefix, name)| cranelift_isle::codegen::Prefix {
            prefix: prefix.display().to_string(),
            name: name.to_string(),
        })
        .collect(),
        // Debug builds cannot rely on rustc shrinking the generated functions'
        // stack frames, so always split aggressively there. This matches what
        // cranelift-codegen's own build script does.
        split_match_arms: cfg!(debug_assertions),
        match_arm_split_threshold: cfg!(debug_assertions).then_some(4),
        ..Default::default()
    };

    let code = match cranelift_isle::compile::from_files(&inputs, &options) {
        Ok(code) => code,
        Err(errors) => {
            eprintln!("{errors}");
            panic!("ISLE compilation failed");
        }
    };

    fs::write(out_dir.join("generated_code.rs"), code)
        .expect("failed writing the generated ISLE code");
}
