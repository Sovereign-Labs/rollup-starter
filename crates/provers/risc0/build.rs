use sov_zkvm_utils::should_skip_guest_build;
use std::collections::HashMap;

fn main() {
    println!("cargo::rerun-if-env-changed=SKIP_GUEST_BUILD");
    println!("cargo::rerun-if-env-changed=OUT_DIR");
    if should_skip_guest_build("risc0") {
        println!("cargo:warning=Skipping risc0 guest build");
        let out_dir = std::env::var_os("OUT_DIR").unwrap();
        let out_dir = std::path::Path::new(&out_dir);
        let methods_path = out_dir.join("methods.rs");

        let elf = r#"
            pub const ROLLUP_PATH: &str = "";
            pub const MOCK_DA_PATH: &str = "";
        "#;

        std::fs::write(methods_path, elf).expect("Failed to write mock rollup elf");
    } else {
        let guest_pkg_to_options = get_guest_options();
        risc0_build::embed_method_metadata_with_options(guest_pkg_to_options);
    }
}

fn get_guest_options() -> HashMap<&'static str, risc0_build::GuestOptions> {
    let mut guest_pkg_to_options = HashMap::new();
    let features = sov_zkvm_utils::collect_features(&["bench"], &["native"]);
    let guest_options = risc0_build::GuestOptionsBuilder::default()
        .features(features)
        .build()
        .unwrap();
    guest_pkg_to_options.insert("guest-mock-starter-risc0", guest_options.clone());
    guest_pkg_to_options.insert("sov-demo-prover-guest-celestia-risc0", guest_options);
    guest_pkg_to_options
}
