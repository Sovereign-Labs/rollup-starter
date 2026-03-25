use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONTRACT_NAME: &str = "StateConsistencyTester";
const CONTRACT_SOURCE: &str = "StateConsistencyTester.sol";
const ABI_FILE: &str = "StateConsistencyTester.abi";
const BIN_FILE: &str = "StateConsistencyTester.bin";
const SOURCE_HASH_FILE: &str = "StateConsistencyTester.sol.sha256";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let contracts_dir = manifest_dir.join("evm-contracts");
    let contract_path = contracts_dir.join(CONTRACT_SOURCE);
    let abi_path = contracts_dir.join(ABI_FILE);
    let bin_path = contracts_dir.join(BIN_FILE);
    let hash_path = contracts_dir.join(SOURCE_HASH_FILE);
    for path in [&contract_path, &abi_path, &bin_path, &hash_path] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let expected_hash = source_hash(&contract_path);
    if !artifacts_are_fresh(&abi_path, &bin_path, &hash_path, &expected_hash) {
        regenerate_artifacts(&contract_path, &contracts_dir, &hash_path, &expected_hash);
    }

    fs::create_dir_all(
        env::var_os("OUT_DIR")
            .map(PathBuf::from)
            .expect("OUT_DIR must be set"),
    )
    .expect("failed to create OUT_DIR");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    copy_artifact(&abi_path, &out_dir.join(ABI_FILE));
    copy_artifact(&bin_path, &out_dir.join(BIN_FILE));
}

fn artifacts_are_fresh(
    abi_path: &Path,
    bin_path: &Path,
    hash_path: &Path,
    expected_hash: &str,
) -> bool {
    abi_path.exists()
        && bin_path.exists()
        && fs::read_to_string(hash_path)
            .map(|hash| hash.trim() == expected_hash)
            .unwrap_or(false)
}

fn regenerate_artifacts(
    contract_path: &Path,
    contracts_dir: &Path,
    hash_path: &Path,
    expected_hash: &str,
) {
    println!(
        "cargo:warning=Regenerating {CONTRACT_NAME} ABI/bin artifacts because the checked-in cache is missing or stale"
    );
    let output = match Command::new("solc")
        .args(["--abi", "--bin"])
        .arg(contract_path)
        .args(["-o", contracts_dir.to_str().unwrap(), "--overwrite"])
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            panic!(
                "{CONTRACT_NAME} artifacts are missing or stale, but `solc` is not available on PATH. Install `solc` and rerun cargo to regenerate the checked-in artifacts under {}",
                contracts_dir.display()
            );
        }
        Err(err) => panic!("failed to run solc: {err}"),
    };

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

    fs::write(hash_path, format!("{expected_hash}\n"))
        .expect("failed to write Solidity source hash");
}

fn copy_artifact(src: &Path, dst: &Path) {
    fs::copy(src, dst).unwrap_or_else(|err| {
        panic!(
            "failed to copy {} to {}: {err}",
            src.display(),
            dst.display()
        )
    });
}

fn source_hash(path: &Path) -> String {
    let bytes =
        fs::read(path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
