// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {StorageTester} from "../src/StorageTester.sol";

contract StorageTests is Script {
    uint256 constant ALTERNATING_ITERATIONS = 20000; // (~10M gas)
    uint256 constant RANDOM_ITERATIONS = 500; // (~10M gas)
    uint256 constant CROSS_TX_ITERATIONS = 100;

    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 2: Storage Tests ===\n");

        StorageTester tester = new StorageTester();
        console2.log("StorageTester deployed at:", address(tester));
        console2.log("");

        testAlternatingReadWrite(tester);
        testRandomSlotWrites(tester);

        vm.stopBroadcast();

        testCrossTxStorage(tester);
    }

    function testAlternatingReadWrite(StorageTester tester) internal {
        console2.log("--- Test 1: Alternating Read-Write (Same Slot) ---");
        console2.log("Operations:", ALTERNATING_ITERATIONS, "(write + read per iteration)");

        uint256 gasBefore = gasleft();
        uint256 checksum = tester.alternatingReadWrite(ALTERNATING_ITERATIONS);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Checksum:", checksum);
        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per operation:", gasUsed / ALTERNATING_ITERATIONS);
        console2.log("");
    }

    function testRandomSlotWrites(StorageTester tester) internal {
        console2.log("--- Test 2: Random Slot Writes (Same Value) ---");
        console2.log("Operations:", RANDOM_ITERATIONS, "writes to pseudo-random slots");

        uint256 gasBefore = gasleft();
        tester.randomSlotWrites(RANDOM_ITERATIONS, 0);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per write:", gasUsed / RANDOM_ITERATIONS);
        console2.log("");
    }

    function testCrossTxStorage(StorageTester tester) internal {
        console2.log("--- Test 3: Cross-Transaction Storage ---");
        console2.log("Iterations:", CROSS_TX_ITERATIONS, "(alternating write/read across tx boundaries)");

        for (uint256 i = 0; i < CROSS_TX_ITERATIONS; i++) {
            vm.startBroadcast();
            tester.setSlot(i);
            vm.stopBroadcast();

            vm.startBroadcast();
            uint256 val = tester.slot();
            require(val == i, "Cross-tx value mismatch");
            vm.stopBroadcast();
        }

        console2.log("Completed", CROSS_TX_ITERATIONS, "write-read cycles across tx boundaries");
        console2.log("");
    }
}
