// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CalldataTester} from "../src/CalldataTester.sol";

contract CalldataTests is Script {
    uint256 constant LARGE_CALLDATA_SIZE = 1024 * 1024; // 1 MB

    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 6: Calldata Tests ===\n");

        CalldataTester tester = new CalldataTester();
        console2.log("CalldataTester deployed at:", address(tester));
        console2.log("");

        testLargeCalldata(tester);

        vm.stopBroadcast();
    }

    function testLargeCalldata(CalldataTester tester) internal {
        console2.log("--- Test: Large Calldata Processing ---");
        console2.log("Calldata size:", LARGE_CALLDATA_SIZE, "bytes (1 MB)");

        bytes memory largeData = new bytes(LARGE_CALLDATA_SIZE);
        for (uint256 i = 0; i < 1024; i++) {
            largeData[i] = bytes1(uint8(i % 256));
        }

        uint256 gasBefore = gasleft();
        uint256 length = tester.processLargeCalldata(largeData);
        uint256 gasUsed = gasBefore - gasleft();

        require(length == LARGE_CALLDATA_SIZE, "Calldata length should match");
        console2.log("Processed bytes:", length);
        console2.log("Total gas used:", gasUsed);
        console2.log("Gas per KB:", gasUsed / (LARGE_CALLDATA_SIZE / 1024));
        console2.log("");
    }
}
