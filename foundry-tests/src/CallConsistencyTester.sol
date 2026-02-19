// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract CallConsistencyTester {
    uint256 public counter;

    function getCounter() public view returns (uint256) {
        return counter;
    }

    function incrementAndReturn() public returns (uint256) {
        counter++;
        return counter;
    }

    function getBlockDependentValue() public view returns (uint256 blockNum, uint256 timestamp, uint256 basefee) {
        blockNum = block.number;
        timestamp = block.timestamp;
        basefee = block.basefee;
    }

    function conditionalOnState() public view returns (string memory) {
        if (counter == 0) return "zero";
        if (counter == 1) return "one";
        return "many";
    }

    function computeHash(bytes memory data) public pure returns (bytes32) {
        return keccak256(data);
    }
}
