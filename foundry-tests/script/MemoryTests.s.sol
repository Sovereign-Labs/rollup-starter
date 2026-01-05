// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {MemoryTester} from "../src/MemoryTester.sol";

contract MemoryTests is Script {
    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 4: Memory Expansion Tests ===\n");

        MemoryTester tester = new MemoryTester();
        console2.log("MemoryTester deployed at:", address(tester));
        console2.log("");

        testIncrementalExpansion(tester);
        testLargeAllocation(tester);

        vm.stopBroadcast();
    }

    function testIncrementalExpansion(MemoryTester tester) internal {
        console2.log("--- Test 1: Incremental Memory Expansion ---");
        console2.log("Steps: 10, Step size: 1024 bytes");

        uint256 gasBefore = gasleft();
        tester.incrementalMemoryExpansion(10, 1024);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Total gas used:", gasUsed);
        console2.log("");
    }

    function testLargeAllocation(MemoryTester tester) internal {
        console2.log("--- Test 2: Large Single Memory Allocation ---");
        console2.log("Allocation size: 1 MB");

        uint256 size = 1024 * 1024;
        uint256 gasBefore = gasleft();
        uint256 length = tester.largeMemoryAllocation(size);
        uint256 gasUsed = gasBefore - gasleft();

        console2.log("Allocated bytes:", length);
        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per MB:", gasUsed / (size / 1024 / 1024));
        console2.log("");
    }
}
