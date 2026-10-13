//@ run-pass
#![feature(type_info, register_tool)]
#![register_tool(meta)]
#![allow(dead_code)]

use std::any::TypeId;
use std::mem::type_info::{crate_functions_with_attr, crate_types_with_attr};

#[path = "auxiliary/typed_items_out_of_line.rs"]
mod typed_items_out_of_line;

#[meta::marked]
struct SampleType {
    retries: u32,
}

#[meta::tagged(group = "alpha")]
fn first_item() {}

#[meta::tagged(group = "beta")]
fn second_item() {}

const MARKED_TYPES: &[TypeId] = crate_types_with_attr("meta::marked");
const ITEMS: &[TypeId] = crate_functions_with_attr("meta::tagged");
const ITEM_GROUPS: [Option<&str>; 2] = [
    ITEMS[0].attr_value("meta::tagged", "group"),
    ITEMS[1].attr_value("meta::tagged", "group"),
];

fn main() {
    assert!(MARKED_TYPES.contains(&TypeId::of::<SampleType>()));
    assert!(MARKED_TYPES.contains(&TypeId::of::<typed_items_out_of_line::OutOfLineType>()));
    assert_eq!(ITEMS.len(), 2);
    assert_eq!(ITEM_GROUPS, [Some("alpha"), Some("beta")]);
}