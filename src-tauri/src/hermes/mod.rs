//! Hermes Agent management (M5). Status is read from files and /proc via the Termux shell
//! (fast, no proot login); actions run through the environment wrapper and supervisor.

pub mod detect;
pub mod env;
pub mod status;
pub mod supervisor;
