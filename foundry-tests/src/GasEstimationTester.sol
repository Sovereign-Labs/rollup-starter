// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract GasEstimationTester {
    event ValueSet(uint256 indexed newValue);

    mapping(uint256 => uint256) public store;
    uint256 public value;

    function setValue(uint256 v) public {
        value = v;
        emit ValueSet(v);
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

}
