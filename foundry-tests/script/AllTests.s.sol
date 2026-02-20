// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {DeploymentTests} from "./DeploymentTests.s.sol";
import {StorageTests} from "./StorageTests.s.sol";
import {EventTests} from "./EventTests.s.sol";
import {MemoryTests} from "./MemoryTests.s.sol";
import {PrecompileTests} from "./PrecompileTests.s.sol";
import {CalldataTests} from "./CalldataTests.s.sol";
import {ContextTests} from "./ContextTests.s.sol";
import {GasEstimationTests} from "./GasEstimationTests.s.sol";
import {ValueTransferTests} from "./ValueTransferTests.s.sol";
import {RevertTests} from "./RevertTests.s.sol";
import {InterContractCallTests} from "./InterContractCallTests.s.sol";

contract AllTests is Script {
    function run() public {
        console2.log("========================================");
        console2.log("Running All EVM Tests");
        console2.log("========================================\n");

        new DeploymentTests().run();
        new StorageTests().run();
        new EventTests().run();
        new MemoryTests().run();
        new PrecompileTests().run();
        new CalldataTests().run();
        new ContextTests().run();
        new GasEstimationTests().run();
        // This suite runs in a dedicated two-phase flow because script simulation can
        // diverge from on-chain observability for tx lifecycle assertions.
        console2.log("Skipping inline RpcTxLifecycle in AllTests; run ./run.sh RpcTxLifecycleFlow for deploy/read RPC lifecycle checks.");
        // SDK bug detector: eth_maxPriorityFeePerGas currently returns 0x00.
        console2.log(
            "Skipping inline RpcFeeAndEstimationSafetyTests in AllTests; SDK returns eth_maxPriorityFeePerGas=0x00. Run ./run.sh RpcFeeAndEstimationSafetyTests."
        );
        // SDK bug detector: eth_getLogs currently returns non-JSON/empty payload shape.
        console2.log(
            "Skipping inline RpcLogAndFilterTests in AllTests; SDK eth_getLogs response shape is incompatible. Run ./run.sh RpcLogAndFilterTests."
        );
        // SDK bug detector: eth_call revert path currently returns empty data (0x).
        console2.log(
            "Skipping inline RpcErrorEnvelopeTests in AllTests; SDK eth_call revert data shape is incompatible. Run ./run.sh RpcErrorEnvelopeTests."
        );
        // SDK bug detector: nonce does not progress after two successful sends.
        console2.log(
            "Skipping inline RpcTagAndNonceMatrixTests in AllTests; SDK nonce progression/pending behavior is incompatible. Run ./run.sh RpcTagAndNonceMatrixTests."
        );
        console2.log("Skipping inline CallConsistency in AllTests; run ./run.sh CallConsistencyFlow for deploy/read RPC checks.");
        new ValueTransferTests().run();
        new RevertTests().run();
        new InterContractCallTests().run();

        console2.log("\n========================================");
        console2.log("All Tests Complete\n");
        console2.log("========================================");
    }
}
