//@ run-pass
//@ edition: 2021
#![feature(type_info, register_tool)]
#![register_tool(meta)]
#![allow(dead_code)]

use std::any::TypeId;
use std::mem::type_info::{FnDef, Type, TypeKind};
use std::mem::type_info::crate_functions_with_attr;

#[meta::reflect(kind = "function", label = "single-parameter")]
fn single_parameter(id: u64) -> Option<String> {
    let _ = id;
    None
}

#[meta::reflect(kind = "function", label = "async")]
async fn async_item(enabled: bool) -> Option<u32> {
    let _ = enabled;
    None
}

const ITEMS: &[TypeId] = crate_functions_with_attr("meta::reflect");
const FUNCTION_ID: TypeId = ITEMS[0];
const ASYNC_ID: TypeId = ITEMS[1];
const FUNCTION_INFO: Type = FUNCTION_ID.info();
const ASYNC_INFO: Type = ASYNC_ID.info();
const HAS_METADATA: bool = FUNCTION_ID.has_attr("meta::reflect");
const ITEM_KIND: Option<&str> = FUNCTION_ID.attr_value("meta::reflect", "kind");
const ITEM_LABEL: Option<&str> = FUNCTION_ID.attr_value("meta::reflect", "label");
const FUNCTION_OUTPUT: Option<TypeId> = match FUNCTION_INFO.kind {
    TypeKind::FnDef(FnDef { output, .. }) => Some(output),
    _ => None,
};
const FUNCTION_DECLARED_OUTPUT: Option<TypeId> = match FUNCTION_INFO.kind {
    TypeKind::FnDef(FnDef { declared_output, .. }) => declared_output,
    _ => None,
};
const ASYNC_DECLARED_OUTPUT: Option<TypeId> = match ASYNC_INFO.kind {
    TypeKind::FnDef(FnDef { declared_output, .. }) => declared_output,
    _ => None,
};

fn main() {
    match FUNCTION_INFO.kind {
        TypeKind::FnDef(FnDef { name, span, inputs, param_names, is_async, .. }) => {
            assert_eq!(name, "single_parameter");
            assert!(span.file.ends_with("function_items.rs"));
            assert!(span.start < span.end);
            assert_eq!(inputs.len(), 1);
            assert_eq!(param_names, [Some("id")]);
            assert!(!is_async);
        }
        _ => panic!("expected function-item reflection"),
    }
    assert_eq!(FUNCTION_OUTPUT, Some(TypeId::of::<Option<String>>()));
    assert_eq!(FUNCTION_DECLARED_OUTPUT, Some(TypeId::of::<Option<String>>()));
    assert_eq!(ASYNC_DECLARED_OUTPUT, Some(TypeId::of::<Option<u32>>()));

    assert!(HAS_METADATA);
    assert_eq!(ITEM_KIND, Some("function"));
    assert_eq!(ITEM_LABEL, Some("single-parameter"));

    match ASYNC_INFO.kind {
        TypeKind::FnDef(FnDef { name, is_async, .. }) => {
            assert_eq!(name, "async_item");
            assert!(is_async);
        }
        _ => panic!("expected function-item reflection"),
    }
}