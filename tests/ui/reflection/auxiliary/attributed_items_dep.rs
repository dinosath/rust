#![feature(register_tool)]
#![register_tool(meta)]

#[meta::select(group = "dependency")]
pub fn dependency_item() {}

pub struct DependencyItems;

impl DependencyItems {
    #[meta::select(group = "associated")]
    pub fn dependency_associated_item() {}
}