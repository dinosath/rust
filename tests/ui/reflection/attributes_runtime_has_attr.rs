#![feature(type_info, register_tool)]
#![register_tool(meta)]

use std::any::TypeId;

#[meta::record]
struct SampleType;

fn main() {
    let has_record = TypeId::of::<SampleType>().has_attr("meta::record");
    //~^ ERROR: comptime functions can only be called at compile time
    let _ = has_record;
}
