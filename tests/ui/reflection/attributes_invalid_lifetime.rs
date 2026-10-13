// A const-time assertion over reflected metadata reports a normal evaluation error.
#![feature(type_info, register_tool)]
#![register_tool(meta)]
#![allow(dead_code)]

use std::any::TypeId;
use std::mem::type_info::crate_types_with_attr;

#[meta::record(group = "context")]
struct Context;

#[meta::record(group = "container")]
struct Container {
    context: Context,
}

const _: () = {
    let records = crate_types_with_attr("meta::record");
    let mut i = 0;
    while i < records.len() {
        let ty = records[i];
        let mut f = 0;
        while f < ty.fields(0) {
            let dep = ty.field(0, f).type_id();
            assert!(!dep.has_attr("meta::record"), "unexpected recorded field type");
            //~^ ERROR evaluation panicked
            f += 1;
        }
        i += 1;
    }
};

fn main() {}
