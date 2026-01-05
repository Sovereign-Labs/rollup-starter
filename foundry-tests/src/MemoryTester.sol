// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract MemoryTester {
    // Test 1: Incremental memory expansion
    function incrementalMemoryExpansion(uint256 steps, uint256 stepSize) public pure {
        for (uint256 i = 0; i < steps; i++) {
            bytes memory data = new bytes(stepSize * (i + 1));

            // Touch the memory to ensure allocation
            if (data.length > 0) {
                data[data.length - 1] = bytes1(uint8(i));
            }
        }
    }

    // Test 2: Large single allocation
    function largeMemoryAllocation(uint256 size) public pure returns (uint256) {
        bytes memory data = new bytes(size);

        // Touch memory to ensure allocation
        data[size - 1] = 0x01;

        return data.length;
    }
}
