// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract CalldataTester {
    function processLargeCalldata(bytes calldata data) public pure returns (uint256) {
        return data.length;
    }
}
