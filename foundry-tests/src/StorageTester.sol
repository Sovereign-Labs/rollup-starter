// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract StorageTester {
    uint256 public slot;

    // Test 1: Alternating read-write with different values to same slot
    function alternatingReadWrite(uint256 iterations) public returns (uint256 checksum) {
        for (uint256 i = 0; i < iterations; i++) {
            // Write new value
            slot = i;
            // Read and verify it changed
            uint256 val = slot;
            require(val == i, "Value mismatch");
            checksum += val;
        }
    }

    // Test 2: Write same value to random slots
    function randomSlotWrites(uint256 iterations, uint256 seed) public {
        uint256 rng = seed;

        for (uint256 i = 0; i < iterations; i++) {
            // Generate pseudo-random slot
            rng = lcg(rng);

            // Write same value (42) to random slot
            assembly {
                sstore(rng, 42)
            }
        }
    }

    // Linear Congruential Generator
    function lcg(uint256 prev) internal pure returns (uint256) {
        return (1664525 * prev + 1013904223) & 0xFFFFFFFF;
    }
}
