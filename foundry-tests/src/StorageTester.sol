// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract StorageTester {
    uint256 public slot;

    function setSlot(uint256 value) public {
        slot = value;
    }

    function alternatingReadWrite(uint256 iterations) public returns (uint256 checksum) {
        for (uint256 i = 0; i < iterations; i++) {
            slot = i;
            uint256 val = slot;
            require(val == i, "Value mismatch");
            checksum += val;
        }
    }

    function randomSlotWrites(uint256 iterations, uint256 seed) public {
        uint256 rng = seed;

        for (uint256 i = 0; i < iterations; i++) {
            rng = lcg(rng);
            assembly {
                sstore(rng, 42)
            }
        }
    }

    function lcg(uint256 prev) internal pure returns (uint256) {
        return (1664525 * prev + 1013904223) & 0xFFFFFFFF;
    }
}
