// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract MemoryTester {
    function incrementalMemoryExpansion(uint256 steps, uint256 stepSize) public pure {
        for (uint256 i = 0; i < steps; i++) {
            bytes memory data = new bytes(stepSize * (i + 1));

            if (data.length > 0) {
                data[data.length - 1] = bytes1(uint8(i));
            }
        }
    }

    function largeMemoryAllocation(uint256 size) public pure returns (uint256) {
        bytes memory data = new bytes(size);
        data[size - 1] = 0x01;
        return data.length;
    }
}
