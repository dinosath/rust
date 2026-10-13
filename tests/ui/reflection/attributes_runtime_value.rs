// Reflection attribute queries are compile-time-only.
#![feature(type_info, register_tool)]
#![register_tool(meta)]

use std::any::TypeId;

#[meta::record(label = "primary")]
struct SampleType;

fn main() {
    let label = TypeId::of::<SampleType>().attr_value("meta::record", "label");
    //~^ ERROR comptime fns can only be called at compile time
    let _ = label;
}
