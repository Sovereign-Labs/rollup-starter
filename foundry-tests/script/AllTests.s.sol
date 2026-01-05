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

        DeploymentTests deploymentTests = new DeploymentTests();
        deploymentTests.run();

        StorageTests storageTests = new StorageTests();
        storageTests.run();

        EventTests eventTests = new EventTests();
        eventTests.run();

        MemoryTests memoryTests = new MemoryTests();
        memoryTests.run();

        PrecompileTests precompileTests = new PrecompileTests();
        precompileTests.run();

        CalldataTests calldataTests = new CalldataTests();
        calldataTests.run();

        ContextTests contextTests = new ContextTests();
        contextTests.run();

        console2.log("\n========================================");
        console2.log("All Tests Complete");
        console2.log("========================================");
    }
}
