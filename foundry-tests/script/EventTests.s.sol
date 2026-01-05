// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {EventTester} from "../src/EventTester.sol";

contract EventTests is Script {
    uint256 constant MANY_LOGS_COUNT = 1000;
    uint256 constant LARGE_DATA_SIZE = 10 * 1024; // 10 KB

    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 3: Event Logging Tests ===\n");

        EventTester tester = new EventTester();
        console2.log("EventTester deployed at:", address(tester));
        console2.log("");

        testAllLogTypes(tester);
        testManyLogs(tester);
        testLargeLog(tester);

        vm.stopBroadcast();
    }

    function testAllLogTypes(EventTester tester) internal {
        console2.log("--- Test 1: All Log Types (LOG0 through LOG4) ---");

        uint256 gasBefore = gasleft();
        tester.emitAllLogTypes();
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Emitted: LOG0, LOG1, LOG2, LOG3, LOG4");
        console2.log("Total gas used:", gasUsed);
        console2.log("Average gas per log:", gasUsed / 5);
        console2.log("");
    }

    function testManyLogs(EventTester tester) internal {
        console2.log("--- Test 2: Many Logs in Single Transaction ---");
        console2.log("Log count:", MANY_LOGS_COUNT);

        uint256 gasBefore = gasleft();
        tester.emitManyLogs(MANY_LOGS_COUNT);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per log:", gasUsed / MANY_LOGS_COUNT);
        console2.log("");
    }

    function testLargeLog(EventTester tester) internal {
        console2.log("--- Test 3: Large Log Data ---");
        console2.log("Data size:", LARGE_DATA_SIZE, "bytes (10 KB)");

        uint256 gasBefore = gasleft();
        tester.emitLargeLog(LARGE_DATA_SIZE);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per byte:", gasUsed / LARGE_DATA_SIZE);
        console2.log("");
    }
}
