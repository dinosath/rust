//@ run-pass
// Attribute reflection plus crate enumeration over arbitrary tool metadata.
#![feature(type_info, register_tool)]
#![register_tool(meta)]
#![allow(dead_code)]

use std::any::TypeId;
use std::mem::type_info::crate_types_with_attr;

#[meta::record(group = "settings", label = "primary")]
struct Settings {
    url: String,
    #[meta::value = 10]
    max_connections: u32,
    #[meta::value = 30]
    connection_timeout_seconds: u64,
}

#[meta::record(group = "item")]
struct Inner {
    settings: Settings,
}

#[meta::record(group = "item")]
struct Outer {
    inner: Inner,
}

struct Plain;

const RECORDS: &[TypeId] = crate_types_with_attr("meta::record");

const _: () = {
    let mut i = 0;
    while i < RECORDS.len() {
        let ty = RECORDS[i];
        let mut f = 0;
        while f < ty.fields(0) {
            let dep = ty.field(0, f).type_id();
            let _ = dep.has_attr("meta::record");
            f += 1;
        }
        i += 1;
    }
};

fn main() {
    assert_eq!(RECORDS, [TypeId::of::<Settings>(), TypeId::of::<Inner>(), TypeId::of::<Outer>()]);

    const GROUP: Option<&str> = RECORDS[0].attr_value("meta::record", "group");
    assert_eq!(GROUP, Some("settings"));
    assert_eq!(const { RECORDS[0].attr_value("meta::record", "missing") }, None);
    assert!(!const { TypeId::of::<Plain>().has_attr("meta::record") });

    const VALUE: Option<&str> =
        TypeId::of::<Settings>().field(0, 1).attr_value("meta::value", "");
    assert_eq!(VALUE, Some("10"));
    assert!(const { !TypeId::of::<Settings>().field(0, 0).has_attr("meta::value") });
}
