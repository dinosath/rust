//@ check-fail
#![feature(type_info)]

use std::mem::type_info::ItemSet;

fn main() {
    let _items = ItemSet::new(&[]);
    //~^ ERROR comptime fns can only be called at compile time
}