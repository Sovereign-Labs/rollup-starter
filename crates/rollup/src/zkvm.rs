#[cfg(feature = "risc0")]
mod risc0 {
    pub use sov_risc0_adapter::host::Risc0Host as ZkvmHost;
    pub use sov_risc0_adapter::Risc0 as Zkvm;
    use sov_rollup_interface::zk::CryptoSpec;
    use std::sync::Arc;

    pub type Hasher = <sov_risc0_adapter::Risc0CryptoSpec as CryptoSpec>::Hasher;

    fn should_skip_guest_build() -> bool {
        match std::env::var("SKIP_GUEST_BUILD")
            .as_ref()
            .map(|arg0: &String| String::as_str(arg0))
        {
            Ok("1") | Ok("true") | Ok("risc0") => true,
            Ok("0") | Ok("false") | Ok(_) | Err(_) => false,
        }
    }

    // Returns the host arguments for a rollup. This is the code that is proven by the rollup
    pub fn rollup_host_args() -> Arc<&'static [u8]> {
        if should_skip_guest_build() {
            return Arc::new(vec![].leak());
        }

        let elf_path: &str;
        if cfg!(feature = "celestia_da") {
            elf_path = risc0_starter::ROLLUP_PATH;
        } else if cfg!(feature = "mock_da") {
            elf_path = risc0_starter::MOCK_DA_PATH;
        } else {
            panic!("No DA feature enabled");
        }

        Arc::new(
            std::fs::read(elf_path)
                .unwrap_or_else(|e| panic!("Could not read guest elf file from `{elf_path}`. {e}"))
                .leak(),
        )
    }

    pub async fn create_inner_vm() -> ZkvmHost<'static> {
        let elf = rollup_host_args();
        ZkvmHost::new(*elf)
    }
}

#[cfg(feature = "sp1")]
mod sp1 {
    use sov_rollup_interface::zk::CryptoSpec;
    pub use sov_sp1_adapter::host::SP1Host as ZkvmHost;
    pub use sov_sp1_adapter::SP1 as Zkvm;
    use std::sync::Arc;
    pub type Hasher = <sov_sp1_adapter::SP1CryptoSpec as CryptoSpec>::Hasher;

    fn should_skip_guest_build() -> bool {
        match std::env::var("SKIP_GUEST_BUILD")
            .as_ref()
            .map(|arg0: &String| String::as_str(arg0))
        {
            Ok("1") | Ok("true") | Ok("sp1") => true,
            Ok("0") | Ok("false") | Ok(_) | Err(_) => false,
        }
    }

    // Returns the host arguments for a rollup. This is the code that is proven by the rollup
    pub fn rollup_host_args() -> Arc<&'static [u8]> {
        if should_skip_guest_build() {
            return Arc::new(vec![].leak());
        }

        if cfg!(feature = "celestia_da") {
            Arc::new(&sp1_starter::SP1_GUEST_CELESTIA_ELF)
        } else if cfg!(feature = "mock_da") {
            Arc::new(&sp1_starter::SP1_GUEST_MOCK_ELF)
        } else {
            panic!("No DA feature enabled");
        }
    }

    pub async fn create_inner_vm() -> ZkvmHost {
        let elf = rollup_host_args();
        // The starter uses `MockZkvm` as the outer zkVM, so SP1's `outer_vk` is a
        // placeholder derived from the inner ELF. Swap in a dedicated aggregation
        // guest key here when wiring SP1 proof aggregation end-to-end.
        //
        // `ProverClient::setup` spins up its own tokio runtime, so the verifying-key
        // derivation and the host construction must run off the async executor thread.
        tokio::task::spawn_blocking(move || {
            let outer_vk = Arc::new(sov_sp1_adapter::host::verifying_key_from_elf(*elf)?);
            ZkvmHost::new(*elf, outer_vk)
        })
        .await
        .expect("SP1Host setup task panicked")
        .expect("Failed to create SP1 host")
    }
}

#[cfg(feature = "mock_zkvm")]
mod mock_zkvm {
    pub use sov_mock_zkvm::MockZkvm as Zkvm;
    pub use sov_mock_zkvm::MockZkvmHost as ZkvmHost;
    use sov_rollup_interface::zk::CryptoSpec;
    use std::sync::Arc;

    pub fn rollup_host_args() -> Arc<()> {
        Arc::new(())
    }

    pub async fn create_inner_vm() -> ZkvmHost {
        ZkvmHost::new()
    }

    pub type Hasher = <sov_mock_zkvm::MockZkvmCryptoSpec as CryptoSpec>::Hasher;
}

#[cfg(feature = "mock_zkvm")]
pub use mock_zkvm::{
    create_inner_vm, rollup_host_args, Hasher, Zkvm as InnerZkvm, ZkvmHost as InnerZkvmHost,
};

#[cfg(feature = "risc0")]
pub use risc0::{
    create_inner_vm, rollup_host_args, Hasher, Zkvm as InnerZkvm, ZkvmHost as InnerZkvmHost,
};

#[cfg(feature = "sp1")]
pub use sp1::{
    create_inner_vm, rollup_host_args, Hasher, Zkvm as InnerZkvm, ZkvmHost as InnerZkvmHost,
};

pub use sov_mock_zkvm::MockZkvm as OuterZkvm;
pub use sov_mock_zkvm::MockZkvmHost as OuterZkvmHost;
use sov_rollup_interface::zk::aggregated_proof::SerializedAggregatedProof;

pub fn get_outer_vm(previous_outer_proof: Option<SerializedAggregatedProof>) -> OuterZkvmHost {
    OuterZkvmHost::new_non_blocking_with_previous_outer_proof(previous_outer_proof)
}
