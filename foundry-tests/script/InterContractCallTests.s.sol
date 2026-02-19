// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {Callee, DelegateeLibrary, Caller} from "../src/InterContractCallTester.sol";

contract InterContractCallTests is Script {
    address constant BROADCAST_SENDER = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    uint256 constant VALUE_CALL_WEI = 1000;

    Callee callee;
    DelegateeLibrary lib;
    Caller caller;

    function run() public {
        vm.startBroadcast();
        console2.log("=== Inter-Contract Call Tests ===\n");

        callee = new Callee();
        lib = new DelegateeLibrary();
        caller = new Caller();

        console2.log("Callee deployed at:", address(callee));
        console2.log("DelegateeLibrary deployed at:", address(lib));
        console2.log("Caller deployed at:", address(caller));
        console2.log("");
        vm.stopBroadcast();

        testCall();
        testDelegateCall();
        testStaticCallRead();
        testStaticCallWriteViolation();
        testNestedCall();
        testCallNonExistent();
        testDeepCalls();
        uint256 senderBalance = getBalance(BROADCAST_SENDER);
        if (senderBalance >= VALUE_CALL_WEI) {
            testCallWithValue();
        } else {
            console2.log(
                "SKIP: sender has insufficient ETH for CALL with value (balance:",
                senderBalance,
                "wei)"
            );
            console2.log("");
        }

        console2.log("=== Inter-Contract Call Tests Complete ===\n");
    }

    function testCall() internal {
        console2.log("--- Test 1: CALL context ---");

        vm.startBroadcast();
        caller.callSetValue(payable(address(callee)), 42);
        vm.stopBroadcast();

        require(callee.value() == 42, "callee value should be 42");
        require(callee.lastCaller() == address(caller), "lastCaller should be caller");
        require(callee.lastOrigin() == tx.origin, "lastOrigin should be tx.origin");
        console2.log("PASS: CALL context correct\n");
    }

    function testDelegateCall() internal {
        console2.log("--- Test 2: DELEGATECALL storage ---");

        vm.startBroadcast();
        caller.delegateCallSetValue(address(lib), 99);
        vm.stopBroadcast();

        require(caller.delegatedValue() == 99, "caller.delegatedValue should be 99");
        // lib's storage should be untouched
        require(lib.delegatedValue() == 0, "lib.delegatedValue should be 0");
        console2.log("PASS: DELEGATECALL wrote to caller's storage\n");
    }

    function testStaticCallRead() internal {
        console2.log("--- Test 3: STATICCALL read ---");

        vm.startBroadcast();
        uint256 val = caller.staticCallGetValue(payable(address(callee)));
        vm.stopBroadcast();

        require(val == 42, "staticcall should return 42");
        console2.log("PASS: STATICCALL read returned correct value\n");
    }

    function testStaticCallWriteViolation() internal {
        console2.log("--- Test 4: STATICCALL write violation ---");

        vm.startBroadcast();
        (bool success,) = caller.staticCallWriteAttempt(address(callee));
        vm.stopBroadcast();

        require(!success, "STATICCALL write should fail");
        console2.log("PASS: STATICCALL write correctly rejected\n");
    }

    function testNestedCall() internal {
        console2.log("--- Test 5: Nested call ---");

        vm.startBroadcast();
        uint256 result = caller.nestedCall(payable(address(callee)), 55);
        vm.stopBroadcast();

        require(result == 55, "nested call should return 55");
        console2.log("PASS: nested call returned correct value\n");
    }

    function testCallNonExistent() internal {
        console2.log("--- Test 6: Call non-existent function ---");

        vm.startBroadcast();
        (bool success, bytes memory data) = caller.callNonExistent(address(0xdEaD));
        vm.stopBroadcast();

        require(success, "call to non-existent should succeed");
        require(data.length == 0, "return data should be empty");
        console2.log("PASS: non-existent call returned success with empty data\n");
    }

    function testDeepCalls() internal {
        console2.log("--- Test 7: Deep recursive calls ---");

        vm.startBroadcast();
        uint256 depth = caller.deepCall(100);
        vm.stopBroadcast();

        require(depth == 100, "depth should be 100");
        console2.log("PASS: deep call reached depth 100\n");
    }

    function testCallWithValue() internal {
        console2.log("--- Test 8: CALL with value ---");

        uint256 balanceBefore = address(callee).balance;

        vm.startBroadcast();
        caller.callWithValue{value: VALUE_CALL_WEI}(address(callee), 7);
        vm.stopBroadcast();

        require(callee.value() == 7, "callee value should be 7");
        require(
            address(callee).balance == balanceBefore + VALUE_CALL_WEI,
            "callee should have received value"
        );
        console2.log("PASS: value transferred through call\n");
    }

    function getBalance(address account) internal returns (uint256) {
        string memory params = string.concat(
            '["', vm.toString(account), '","latest"]'
        );
        bytes memory rpcResult = vm.rpc("eth_getBalance", params);
        return decodeRpcQuantity(rpcResult);
    }

    function decodeRpcQuantity(bytes memory raw) internal pure returns (uint256 value) {
        require(raw.length > 0, "rpc quantity must not be empty");
        require(raw.length <= 32, "rpc quantity too large");
        for (uint256 i = 0; i < raw.length; i++) {
            value = (value << 8) | uint8(raw[i]);
        }
    }
}
