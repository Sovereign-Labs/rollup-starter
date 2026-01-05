// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {DeploymentTests} from "./DeploymentTests.s.sol";
import {StorageTests} from "./StorageTests.s.sol";
import {EventTests} from "./EventTests.s.sol";

/**
 * @title AllTests
 * @notice Umbrella script that runs all test suites
 */
contract AllTests is Script {
    function run() public {
        console2.log("========================================");
        console2.log("Running All EVM Tests");
        console2.log("========================================\n");

        // Phase 1: Deployment Tests
        DeploymentTests deploymentTests = new DeploymentTests();
        deploymentTests.run();

        // Phase 2: Storage Tests
        StorageTests storageTests = new StorageTests();
        storageTests.run();

        // Phase 3: Event Tests
        EventTests eventTests = new EventTests();
        eventTests.run();

        console2.log("\n========================================");
        console2.log("All Tests Complete");
        console2.log("========================================");
    }
}
