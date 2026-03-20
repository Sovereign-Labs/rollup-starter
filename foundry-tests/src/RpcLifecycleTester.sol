// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract RpcLifecycleTester {
    event ValueUpdated(address indexed caller, uint256 indexed newValue, uint256 oldValue);
    event TopicEvent(address indexed caller, uint256 indexed id, string message);
    event CounterBumped(address indexed caller, uint256 indexed newCounter);

    uint256 public value;
    uint256 public counter;

    function updateValue(uint256 newValue) external returns (uint256 oldValue) {
        oldValue = value;
        value = newValue;
        emit ValueUpdated(msg.sender, newValue, oldValue);
    }

    function emitTopicEvent(uint256 id, string calldata message) external {
        emit TopicEvent(msg.sender, id, message);
    }

    function bumpCounter() external returns (uint256) {
        counter += 1;
        emit CounterBumped(msg.sender, counter);
        return counter;
    }
}
