use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let contracts_dir = manifest_dir.join("evm-contracts");
    let contract_path = contracts_dir.join("StateConsistencyTester.sol");
    println!("cargo:rerun-if-changed={}", contracts_dir.display());
    println!("cargo:rerun-if-changed={}", contract_path.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let output = Command::new("solc")
        .args(["--abi", "--bin"])
        .arg(&contract_path)
        .args(["-o", out_dir.to_str().unwrap(), "--overwrite"])
        .output()
        .expect("failed to run solc");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "solc failed (exit {:?}). stdout: {} stderr: {}",
            output.status.code(),
            stdout,
            stderr
        );
    }
}
