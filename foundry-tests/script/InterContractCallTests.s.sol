// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {Callee, DelegateeLibrary, Caller} from "../src/InterContractCallTester.sol";

contract InterContractCallTests is Script {
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
        testCallWithValue();

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
        caller.callWithValue{value: 1000}(address(callee), 7);
        vm.stopBroadcast();

        require(callee.value() == 7, "callee value should be 7");
        require(
            address(callee).balance == balanceBefore + 1000,
            "callee should have received 1000 wei"
        );
        console2.log("PASS: value transferred through call\n");
    }
}
