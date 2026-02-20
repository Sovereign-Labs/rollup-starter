// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {Callee, DelegateeLibrary, Caller} from "../src/InterContractCallTester.sol";

contract InterContractCallTests is Script {
    uint256 constant VALUE_CALL_WEI = 1000;

    Callee callee;
    DelegateeLibrary lib;
    Caller caller;
    address broadcaster;

    function run() public {
        vm.startBroadcast();
        console2.log("=== Inter-Contract Call Tests ===\n");
        broadcaster = tx.origin;

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
        uint256 senderBalance = getBalance(broadcaster);
        if (senderBalance >= VALUE_CALL_WEI) {
            testCallWithValue();
        } else {
            console2.log("SKIP: sender has insufficient ETH for CALL with value");
            console2.log("sender:", broadcaster);
            console2.log("balance:", senderBalance, "wei");
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

    function decodeRpcQuantity(bytes memory raw) internal pure returns (uint256) {
        require(raw.length > 0, "rpc quantity must not be empty");

        if (isHexStringBytes(raw)) {
            return parseHexQuantity(raw);
        }

        (bool ok, bytes memory decodedString) = tryDecodeAbiEncodedHexString(raw);
        if (ok) {
            return parseHexQuantity(decodedString);
        }

        require(raw.length <= 32, "rpc quantity too large");
        uint256 value;
        for (uint256 i = 0; i < raw.length; i++) {
            value = (value << 8) | uint8(raw[i]);
        }
        return value;
    }

    function isHexStringBytes(bytes memory raw) internal pure returns (bool) {
        return raw.length >= 2 && raw[0] == 0x30 && raw[1] == 0x78;
    }

    function parseHexQuantity(bytes memory raw) internal pure returns (uint256 value) {
        require(raw.length >= 2, "invalid hex quantity");
        require(raw[0] == 0x30 && raw[1] == 0x78, "hex quantity must start with 0x");
        for (uint256 i = 2; i < raw.length; i++) {
            uint8 c = uint8(raw[i]);
            uint8 nibble;
            if (c >= 48 && c <= 57) {
                nibble = c - 48;
            } else if (c >= 97 && c <= 102) {
                nibble = c - 87;
            } else if (c >= 65 && c <= 70) {
                nibble = c - 55;
            } else {
                revert("invalid hex digit");
            }
            value = (value << 4) | uint256(nibble);
        }
    }

    function tryDecodeAbiEncodedHexString(bytes memory raw) internal pure returns (bool, bytes memory) {
        if (raw.length < 96 || raw.length % 32 != 0) {
            return (false, bytes(""));
        }

        uint256 offset;
        uint256 len;
        assembly {
            offset := mload(add(raw, 0x20))
            len := mload(add(raw, 0x40))
        }
        if (offset != 0x20 || len < 2) {
            return (false, bytes(""));
        }

        uint256 paddedLen = ((len + 31) / 32) * 32;
        if (raw.length != 64 + paddedLen) {
            return (false, bytes(""));
        }

        bytes memory decoded = new bytes(len);
        for (uint256 i = 0; i < len; i++) {
            decoded[i] = raw[64 + i];
        }
        if (!isHexStringBytes(decoded)) {
            return (false, bytes(""));
        }

        return (true, decoded);
    }
}
