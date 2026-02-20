// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract Callee {
    uint256 public value;
    address public lastCaller;
    address public lastOrigin;

    function setValueAndRecordContext(uint256 v) public payable {
        value = v;
        lastCaller = msg.sender;
        lastOrigin = tx.origin;
    }

    function getValue() public view returns (uint256) {
        return value;
    }

    function writeInView() public returns (uint256) {
        // Not actually view — contains sstore that will fail under STATICCALL
        assembly {
            sstore(0, 1)
        }
        return 1;
    }

    receive() external payable {}
}

contract DelegateeLibrary {
    // slot 0 matches Caller.delegatedValue
    uint256 public delegatedValue;

    function setValueViaDelegate(uint256 v) public {
        delegatedValue = v;
    }

}

contract Caller {
    uint256 public delegatedValue; // slot 0, matches DelegateeLibrary.delegatedValue

    function callSetValue(address payable callee, uint256 v) public {
        Callee(callee).setValueAndRecordContext(v);
    }

    function delegateCallSetValue(address lib, uint256 v) public {
        (bool success,) = lib.delegatecall(
            abi.encodeWithSignature("setValueViaDelegate(uint256)", v)
        );
        require(success, "delegatecall failed");
    }

    function staticCallGetValue(address payable callee) public view returns (uint256) {
        return Callee(callee).getValue();
    }

    function staticCallWriteAttempt(address callee) public returns (bool success, bytes memory data) {
        (success, data) = callee.staticcall(
            abi.encodeWithSignature("writeInView()")
        );
    }

    function nestedCall(address payable callee, uint256 v) public returns (uint256) {
        Callee(callee).setValueAndRecordContext(v);
        return Callee(callee).getValue();
    }

    function callNonExistent(address target) public returns (bool success, bytes memory data) {
        (success, data) = target.call(
            abi.encodeWithSignature("nonExistentFunction()")
        );
    }

    function deepCall(uint256 maxDepth) public returns (uint256) {
        if (maxDepth <= 1) return 1;
        (bool success, bytes memory data) = address(this).call(
            abi.encodeWithSignature("deepCall(uint256)", maxDepth - 1)
        );
        require(success, "deep call failed");
        return abi.decode(data, (uint256)) + 1;
    }

    function callWithValue(address callee, uint256 v) public payable {
        Callee(payable(callee)).setValueAndRecordContext{value: msg.value}(v);
    }

    receive() external payable {}
}
