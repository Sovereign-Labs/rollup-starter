use alloy_sol_types::sol;

sol!(
    #[sol(
        all_derives = true,
        bytecode = include_str!(concat!(
            env!("OUT_DIR"),
            "/StateConsistencyTester.bin"
        ))
    )]
    StateConsistencyTester,
    concat!(env!("OUT_DIR"), "/StateConsistencyTester.abi")
);
