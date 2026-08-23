//! Local input, dotenv, output, and process safety boundaries.

mod dotenv;
mod input;
mod output;
mod process;

pub use dotenv::{
    DotenvMergeMode, DotenvMergeSummary, ParsedEnvironment, merge_dotenv, select_dotenv,
};
pub use input::read_bounded;
pub use output::{PrivateOutputOptions, write_private_atomic};
pub use process::{EnvironmentMode, ManagedChild, spawn_child, wait_child_forwarding_interrupt};
