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

/**
 * @title AllTests
 * @notice Umbrella script that runs all test suites
 */
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

        console2.log("\n========================================");
        console2.log("All Tests Complete\n");
        console2.log("========================================");
    }
}
