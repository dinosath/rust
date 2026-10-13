//@ run-pass
//@ edition: 2021
//@ aux-build: attributed_items_dep.rs
#![feature(type_info, register_tool, rustc_attrs)]
#![register_tool(meta)]
#![allow(internal_features)]
#![allow(dead_code)]

use std::any::TypeId;
use std::mem::type_info::{all_functions_with_attr, FnDef, TypeKind};

extern crate attributed_items_dep;

#[meta::select(group = "local")]
fn local_item() {}

const ITEMS: &[TypeId] = all_functions_with_attr("meta::select");

#[rustc_comptime]
fn item_names() -> [&'static str; ITEMS.len()] {
    let mut names = [""; ITEMS.len()];
    let mut index = 0;
    while index < ITEMS.len() {
        let TypeKind::FnDef(FnDef { name, .. }) = ITEMS[index].info().kind else {
            panic!("expected function item");
        };
        names[index] = name;
        index += 1;
    }
    names
}

const ITEM_NAMES: [&str; ITEMS.len()] = item_names();

fn main() {
    let mut found_local = false;
    let mut found_dependency = false;
    let mut found_dependency_associated = false;

    for name in ITEM_NAMES {
        match name {
            "local_item" => found_local = true,
            "dependency_item" => found_dependency = true,
            "dependency_associated_item" => found_dependency_associated = true,
            _ => {}
        }
    }

    assert!(found_local);
    assert!(found_dependency);
    assert!(found_dependency_associated);
}