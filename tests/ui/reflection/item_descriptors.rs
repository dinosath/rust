//@ run-pass
#![feature(type_info, rustc_attrs)]
#![allow(dead_code)]
#![allow(internal_features)]

use std::any::TypeId;
use std::mem::type_info::{
    BinaryOp, Expression, Function, Item, ItemKind, ItemSet, ItemVisibility, Parameter, Static,
};

struct Counter {
    value: u32,
}

const PARAMS: &[Parameter] = &[Parameter::new("value", TypeId::of::<u32>())];
const INCREMENT_BODY: Expression = Expression::Binary {
    op: BinaryOp::Add,
    lhs: &Expression::Parameter(0),
    rhs: &Expression::Integer { ty: TypeId::of::<u32>(), bits: 1 },
};
const ZERO_FIELD: Expression = Expression::Integer { ty: TypeId::of::<u32>(), bits: 0 };
const INITIAL_FIELDS: &[Expression] = &[ZERO_FIELD];
const ITEMS: &[Item] = &[
    Item::new(
        &[],
        ItemVisibility::Public,
        ItemKind::Function(Function::new(
            "increment",
            PARAMS,
            TypeId::of::<u32>(),
            false,
            INCREMENT_BODY,
        )),
    ),
    Item::new(
        &["generated"],
        ItemVisibility::Crate,
        ItemKind::Static(Static::new(
            "INITIAL_COUNTER",
            TypeId::of::<Counter>(),
            Expression::Construct {
                ty: TypeId::of::<Counter>(),
                fields: INITIAL_FIELDS,
            },
            false,
        )),
    ),
];
const GENERATED_ITEMS: ItemSet = build_items();

#[rustc_comptime]
fn build_items() -> ItemSet {
    ItemSet::new(ITEMS)
}

fn main() {
    assert_eq!(GENERATED_ITEMS.items.len(), 2);
    let ItemKind::Function(function) = GENERATED_ITEMS.items[0].kind else {
        panic!("expected function descriptor");
    };
    assert_eq!(function.name, "increment");
    assert_eq!(function.parameters[0].ty, TypeId::of::<u32>());
    assert_eq!(function.output, TypeId::of::<u32>());
    assert!(matches!(function.body, Expression::Binary { op: BinaryOp::Add, .. }));

    let ItemKind::Static(static_item) = GENERATED_ITEMS.items[1].kind else {
        panic!("expected static descriptor");
    };
    assert_eq!(GENERATED_ITEMS.items[1].module, ["generated"]);
    assert_eq!(GENERATED_ITEMS.items[1].visibility, ItemVisibility::Crate);
    assert_eq!(static_item.ty, TypeId::of::<Counter>());
}