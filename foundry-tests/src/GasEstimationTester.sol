// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract GasEstimationTester {
    mapping(uint256 => uint256) public store;
    uint256 public value;

    function setValue(uint256 v) public {
        value = v;
    }

    function getValue() public view returns (uint256) {
        return value;
    }

    function multiStore(uint256 count) public {
        for (uint256 i = 0; i < count; i++) {
            store[i] = i;
        }
    }

    function fibonacci(uint256 n) public pure returns (uint256) {
        if (n <= 1) return n;
        uint256 a = 0;
        uint256 b = 1;
        for (uint256 i = 2; i <= n; i++) {
            uint256 c = a + b;
            a = b;
            b = c;
        }
        return b;
    }

    function revertIf(bool shouldRevert) public pure {
        require(!shouldRevert, "Forced revert");
    }
}
