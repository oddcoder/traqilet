#![allow(
    clippy::all,
    clippy::pedantic,
    unused_imports,
    unused_variables,
    unused_mut,
    unreachable_patterns,
    non_snake_case
)]
//! ISLE integration glue for BPF lowering.
use cranelift_codegen::ir::condcodes::*;
use cranelift_codegen::ir::immediates::*;
use cranelift_codegen::ir::*;
use cranelift_codegen::machinst::isle::*;
use cranelift_codegen::machinst::{
    ArgPair, CallArgList, CallRetList, InstOutput, MachLabel, Reg, RetPair, VCodeConstant,
};
use regalloc2::PReg;

type BoxExternalName = Box<ExternalName>;

include!(concat!(env!("OUT_DIR"), "/generated_code.rs"));
