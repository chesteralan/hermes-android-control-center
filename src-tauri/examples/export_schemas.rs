use std::fs;
use std::path::{Path, PathBuf};

use schemars::{schema_for, JsonSchema};
use serde::Serialize;

fn write_schema<T: JsonSchema + Serialize>(directory: &Path, name: &str) {
    let schema = schema_for!(T);
    let path = directory.join(format!("{name}.schema.json"));
    fs::write(
        path,
        serde_json::to_vec_pretty(&schema).expect("serialize JSON schema"),
    )
    .expect("write JSON schema");
}

fn main() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../android/hermes-control/schema");
    fs::create_dir_all(&directory).expect("create schema directory");
    write_schema::<hacc_lib::transport::CommandResult>(&directory, "CommandResult");
    write_schema::<hacc_lib::transport::StreamEvent>(&directory, "StreamEvent");
    write_schema::<hacc_lib::hermes::status::HermesStatus>(&directory, "HermesStatus");
    write_schema::<hacc_lib::logs::LogLine>(&directory, "LogLine");
    write_schema::<hacc_lib::error::ErrorPayload>(&directory, "ErrorPayload");
}
