// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract StateConsistencyTester {
    uint256 public value;

    event Updated(uint256 oldValue, uint256 newValue);

    function update(uint256 oldValue, uint256 newValue) external {
        require(oldValue == value, "old value mismatch");
        value = newValue;
        emit Updated(oldValue, newValue);
    }
}
