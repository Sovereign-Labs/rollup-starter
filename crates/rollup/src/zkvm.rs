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

    pub fn create_inner_vm_from_config(
        prover_config: sov_stf_runner::processes::RollupProverConfig<Zkvm>,
    ) -> ZkvmHost<'static> {
        let (elf, _) = prover_config.split();
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

    pub fn create_inner_vm_from_config(
        prover_config: sov_stf_runner::processes::RollupProverConfig<Zkvm>,
    ) -> ZkvmHost {
        let (elf, _) = prover_config.split();
        ZkvmHost::new(*elf).expect("Failed to create SP1 host")
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

    pub fn create_inner_vm_from_config(
        _prover_config: sov_stf_runner::processes::RollupProverConfig<Zkvm>,
    ) -> ZkvmHost {
        // Mock zkvm doesn't need the ELF from prover config
        ZkvmHost::new()
    }

    pub type Hasher = <sov_mock_zkvm::MockZkvmCryptoSpec as CryptoSpec>::Hasher;
}

#[cfg(feature = "mock_zkvm")]
pub use mock_zkvm::{
    create_inner_vm_from_config, rollup_host_args, Hasher, Zkvm as InnerZkvm,
    ZkvmHost as InnerZkvmHost,
};

#[cfg(feature = "risc0")]
pub use risc0::{
    create_inner_vm_from_config, rollup_host_args, Hasher, Zkvm as InnerZkvm,
    ZkvmHost as InnerZkvmHost,
};

#[cfg(feature = "sp1")]
pub use sp1::{
    create_inner_vm_from_config, rollup_host_args, Hasher, Zkvm as InnerZkvm,
    ZkvmHost as InnerZkvmHost,
};

pub use sov_mock_zkvm::MockZkvm as OuterZkvm;
pub use sov_mock_zkvm::MockZkvmHost as OuterZkvmHost;

pub fn get_outer_vm() -> OuterZkvmHost {
    OuterZkvmHost::new_non_blocking()
}
