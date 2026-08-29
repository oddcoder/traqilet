use std::collections::BTreeMap;
use traqilet_btf::{
    Btf, TypeKind,
    uapi::{TypeId, VarLinkage},
};

/// The BTF of an Ubuntu 26.04 amd64 kernel; see `blobs/README.md`.
const KERNEL_BTF: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../blobs/ubuntu-26.04-amd64.btf"
);

const KINDS: &[(&str, u32)] = &[
    ("Array", 4105),
    ("Const", 3960),
    ("DataSec", 1),
    ("DeclTag", 274),
    ("Enum", 2807),
    ("Enum64", 22),
    ("Float", 1),
    ("Func", 69509),
    ("FuncProto", 44004),
    ("Fwd", 117),
    ("Int", 15),
    ("Ptr", 25920),
    ("Restrict", 10),
    ("SEnum", 66),
    ("Struct", 13050),
    ("TypeTag", 1),
    ("Typedef", 2700),
    ("Union", 2245),
    ("Var", 444),
    ("Volatile", 24),
];

fn kernel() -> Btf {
    Btf::from_file(KERNEL_BTF).expect("the committed kernel BTF should parse")
}

fn name_of(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Void => "Void",
        TypeKind::Ptr { .. } => "Ptr",
        TypeKind::Typedef { .. } => "Typedef",
        TypeKind::Int { .. } => "Int",
        TypeKind::Func { .. } => "Func",
        TypeKind::Const { .. } => "Const",
        TypeKind::Volatile { .. } => "Volatile",
        TypeKind::Restrict { .. } => "Restrict",
        TypeKind::TypeTag { .. } => "TypeTag",
        TypeKind::Fwd { .. } => "Fwd",
        TypeKind::Float { .. } => "Float",
        TypeKind::FuncProto { .. } => "FuncProto",
        TypeKind::Array { .. } => "Array",
        TypeKind::Struct { .. } => "Struct",
        TypeKind::Union { .. } => "Union",
        TypeKind::Enum { .. } => "Enum",
        TypeKind::SEnum { .. } => "SEnum",
        TypeKind::Enum64 { .. } => "Enum64",
        TypeKind::SEnum64 { .. } => "SEnum64",
        TypeKind::Var { .. } => "Var",
        TypeKind::DataSec { .. } => "DataSec",
        TypeKind::DeclTag { .. } => "DeclTag",
    }
}

fn kinds(btf: &Btf) -> impl Iterator<Item = (TypeId, &TypeKind, &str)> {
    (1..btf.len() as u32).map(TypeId).map(move |id| {
        let ty = btf.get(id).expect("every id below len has a type");

        (id, &ty.kind, btf.string_at(ty.name_off))
    })
}

#[test]
fn every_entry_of_the_type_section_is_read() {
    let btf = kernel();

    assert_eq!(btf.len(), 169_276);
    assert!(!btf.is_empty());
}

#[test]
fn the_kinds_come_out_as_bpftool_counts_them() {
    let btf = kernel();
    let mut counted: BTreeMap<&str, u32> = BTreeMap::new();
    for (_, kind, _) in kinds(&btf) {
        *counted.entry(name_of(kind)).or_default() += 1;
    }

    let expected: BTreeMap<&str, u32> = KINDS.iter().copied().collect();
    assert_eq!(counted, expected);
}

#[test]
fn a_function_reaches_its_params_through_its_proto() {
    let btf = kernel();
    let proto = btf
        .find_all("vfs_read")
        .iter()
        .filter_map(|id| btf.get(*id))
        .find_map(|ty| match ty.kind {
            TypeKind::Func { proto } => Some(proto),
            _ => None,
        })
        .expect("vfs_read is a function here");

    let TypeKind::FuncProto { ret, params } = &btf.get(proto).expect("the proto is a type").kind
    else {
        panic!("a Func points at a FuncProto");
    };

    let names: Vec<_> = params.iter().map(|p| btf.string_at(p.name_off)).collect();
    assert_eq!(names, ["file", "buf", "count", "pos"]);
    assert_ne!(*ret, TypeId(0), "vfs_read returns something");
    assert!(
        params.iter().all(|p| p.type_id != TypeId(0)),
        "none variadic"
    );
}

#[test]
fn a_variable_keeps_the_linkage_it_was_declared_with() {
    let btf = kernel();
    let (_, kind, _) = kinds(&btf)
        .find(|(_, kind, name)| matches!(kind, TypeKind::Var { .. }) && *name == "__irq_regs")
        .expect("__irq_regs is a per-cpu variable");

    let TypeKind::Var { type_id, linkage } = kind else {
        unreachable!()
    };
    assert_eq!(*linkage, VarLinkage::GlobalAllocated);
    assert_ne!(*type_id, TypeId(0), "a variable has a type");
}

#[test]
fn an_enum_carries_its_constants() {
    let btf = kernel();
    let (_, kind, _) = kinds(&btf)
        .find(|(_, kind, name)| matches!(kind, TypeKind::Enum { .. }) && *name == "bpf_map_type")
        .expect("the kernel describes its own map kinds");

    let TypeKind::Enum { size, values } = kind else {
        unreachable!()
    };
    assert_eq!(*size, 4);
    assert_eq!(values.len(), 38);

    let hash = values
        .iter()
        .find(|v| btf.string_at(v.name_off) == "BPF_MAP_TYPE_HASH")
        .expect("BPF_MAP_TYPE_HASH is one of them");
    assert_eq!(hash.val, 1);
}

#[test]
fn a_64_bit_enumerator_too_large_for_an_i64_survives() {
    let btf = kernel();
    let biggest = kinds(&btf)
        .filter_map(|(_, kind, _)| match kind {
            TypeKind::Enum64 { values, .. } => Some(values),
            _ => None,
        })
        .flatten()
        .find(|v| btf.string_at(v.name_off) == "PT_VADDR_MAX")
        .expect("PT_VADDR_MAX is an unsigned 64 bit enumerator");

    assert_eq!(biggest.val, u64::MAX);
}

#[test]
fn a_bitfield_member_keeps_its_width_and_the_rest_do_not() {
    let btf = kernel();
    let (_, kind, _) = kinds(&btf)
        .find(|(_, kind, name)| matches!(kind, TypeKind::Struct { .. }) && *name == "Scsi_Host")
        .expect("Scsi_Host has bitfields");

    let TypeKind::Struct { size, members } = kind else {
        unreachable!()
    };
    assert_eq!(*size, 2304);

    let active_mode = members
        .iter()
        .find(|m| btf.string_at(m.name_off) == "active_mode")
        .expect("active_mode is one of its bitfields");
    assert_eq!(active_mode.bit_offset, 4672);
    assert_eq!(active_mode.bitfield_size, Some(2));

    assert!(
        members.iter().any(|m| m.bitfield_size.is_none()),
        "not every member of it is a bitfield",
    );
}

#[test]
fn an_array_counts_elements_and_names_what_indexes_it() {
    let btf = kernel();
    let TypeKind::Array {
        elem_type_id,
        index_type_id,
        nelems,
    } = btf.get(TypeId(3)).expect("id 3 is an array").kind
    else {
        panic!("id 3 is an array in this file");
    };

    assert_eq!(nelems, 2);
    assert_eq!(
        btf.string_at(btf.get(elem_type_id).unwrap().name_off),
        "long unsigned int"
    );
    assert_ne!(index_type_id, TypeId(0), "something indexes it");
}

#[test]
fn an_ints_encoding_says_how_to_read_it() {
    let btf = kernel();
    let ints: BTreeMap<&str, (u32, bool, bool, bool)> = kinds(&btf)
        .filter_map(|(_, kind, name)| match kind {
            TypeKind::Int { size, encoding } => Some((
                name,
                (
                    *size,
                    encoding.is_signed(),
                    encoding.is_char(),
                    encoding.is_bool(),
                ),
            )),
            _ => None,
        })
        .collect();

    assert_eq!(ints["_Bool"], (1, false, false, true));
    assert_eq!(ints["signed char"], (1, true, false, false));
    assert_eq!(ints["char"], (1, false, false, false));
    assert_eq!(ints["long unsigned int"], (8, false, false, false));
}

#[test]
fn a_decl_tag_points_at_what_it_annotates() {
    let btf = kernel();
    let tags: Vec<_> = kinds(&btf)
        .filter_map(|(_, kind, name)| match kind {
            TypeKind::DeclTag {
                target,
                component_idx,
            } => Some((name, *target, *component_idx)),
            _ => None,
        })
        .collect();

    assert_eq!(tags.len(), 274);
    assert_eq!(tags[0], ("bpf_fastcall", TypeId(97671), -1));
    assert!(
        tags.iter().all(|(_, _, idx)| *idx == -1),
        "every tag here is on a declaration rather than one of its parts",
    );
}

#[test]
fn the_only_type_tag_is_the_arena_one() {
    let btf = kernel();
    let tags: Vec<_> = kinds(&btf)
        .filter_map(|(_, kind, name)| match kind {
            TypeKind::TypeTag { target } => Some((name, *target)),
            _ => None,
        })
        .collect();

    assert_eq!(tags, [("address_space(1)", TypeId(0))]);
}

#[test]
fn the_only_data_section_is_the_per_cpu_one() {
    let btf = kernel();
    let sections: Vec<_> = kinds(&btf)
        .filter_map(|(_, kind, name)| match kind {
            TypeKind::DataSec { size, vars } => Some((name, *size, vars.len())),
            _ => None,
        })
        .collect();

    assert_eq!(sections, [(".data..percpu", 221_440, 444)]);
}

#[test]
fn a_struct_is_found_by_name_and_has_its_members() {
    let btf = kernel();
    let ids = btf.find_all("task_struct");
    assert!(!ids.is_empty(), "the kernel describes its own task_struct");

    let (size, members) = ids
        .iter()
        .filter_map(|id| btf.get(*id))
        .find_map(|ty| match &ty.kind {
            TypeKind::Struct { size, members } => Some((*size, members)),
            _ => None,
        })
        .expect("one of them is the definition");

    assert!(size > 1000, "a task_struct is not small: {size}");
    assert!(
        members.iter().any(|m| btf.string_at(m.name_off) == "pid"),
        "a task_struct has a pid",
    );
}
