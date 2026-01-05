// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {PrecompileTester} from "../src/PrecompileTester.sol";

contract PrecompileTests is Script {
    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 5: Precompile Tests ===\n");

        PrecompileTester tester = new PrecompileTester();
        console2.log("PrecompileTester deployed at:", address(tester));
        console2.log("");

        testIdentityPrecompile(tester);

        vm.stopBroadcast();
    }

    function testIdentityPrecompile(PrecompileTester tester) internal {
        console2.log("--- Test: Identity Precompile (0x04) ---");

        uint256 gasBefore = gasleft();
        (bool success, bytes memory result) = tester.testIdentityPrecompile();
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Call succeeded:", success);
        console2.log("Result length:", result.length, "bytes");
        console2.log("Total gas used:", gasUsed);
        console2.log("");
    }
}
