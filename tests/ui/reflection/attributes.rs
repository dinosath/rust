//@ run-pass

#![feature(type_info)]
#![feature(register_tool)]
#![register_tool(my_tool)]
#![allow(dead_code)]

use std::mem::type_info::{Type, TypeKind};

#[allow(dead_code)]
#[doc = "A test struct"]
struct AttrStruct {
    #[allow(unused)]
    first: u8,
    second: u16,
}

#[rustfmt::skip]
struct ToolAttr {
    x: u8,
}

// Exercise AttrArgs::Eq path with a custom tool attribute.
#[my_tool::category = "test_value"]
struct EqAttr {
    x: u8,
}

fn main() {
    // Verify basic attribute reflection on a struct with mixed attributes.
    // #[doc = "..."] is parsed by the compiler into DocComment, so only #[allow] is reflected.
    let Type { kind: TypeKind::Struct(ty), .. } = (const { Type::of::<AttrStruct>() }) else {
        panic!()
    };
    assert_eq!(ty.attributes.len(), 1);
    assert_eq!(ty.attributes[0].path, "allow");
    assert_eq!(ty.attributes[0].args, "dead_code");
    assert_eq!(ty.fields[0].attributes.len(), 1);
    assert_eq!(ty.fields[0].attributes[0].path, "allow");

    // Verify namespaced (tool) attribute paths.
    let Type { kind: TypeKind::Struct(ty), .. } = (const { Type::of::<ToolAttr>() }) else {
        panic!()
    };
    assert_eq!(ty.attributes.len(), 1);
    assert_eq!(ty.attributes[0].path, "rustfmt::skip");
    assert_eq!(ty.attributes[0].args, "");

    // Verify AttrArgs::Eq path with custom tool attribute.
    let Type { kind: TypeKind::Struct(ty), .. } = (const { Type::of::<EqAttr>() }) else {
        panic!()
    };
    assert_eq!(ty.attributes.len(), 1);
    assert_eq!(ty.attributes[0].path, "my_tool::category");
    assert_eq!(ty.attributes[0].args, "test_value");

    // Verify cross-crate type attributes are accessible.
    let Type { kind: TypeKind::Enum(ty), .. } = (const { Type::of::<Option<i32>>() }) else {
        panic!()
    };
    let _ = ty.attributes;
    for v in ty.variants {
        let _ = v.attributes;
    }
}
