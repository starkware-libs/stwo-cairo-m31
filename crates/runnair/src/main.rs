use vm::run_fibonacci;

pub mod adapter;
pub mod memory;
pub mod utils;
pub mod vm;

fn main() {
    run_fibonacci();
}
