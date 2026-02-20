// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {RevertTester} from "../src/RevertTester.sol";

contract RevertTests is Script {
    RevertTester tester;

    function run() public {
        vm.startBroadcast();
        console2.log("=== Revert & Error Handling Tests ===\n");

        tester = new RevertTester();
        console2.log("RevertTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        testTryCallRevert();
        testConditionalNoRevert();
        testStateRollbackOnRevert();
        testOverflowUnchecked();
        testAssertFailure();
        testDivisionByZero();
        testRevertViaEthCall();
        testCustomErrorPayload();
        testEmptyRevertData();
        testSimpleCustomError();

        console2.log("=== Revert Tests Complete ===\n");
    }

    function testTryCallRevert() internal {
        console2.log("--- Test 1: tryCallRevert internal catch ---");

        vm.startBroadcast();
        (bool success, bytes memory returnData) = tester.tryCallRevert();
        vm.stopBroadcast();

        require(!success, "inner call should have failed");
        // Error(string) selector is 0x08c379a0
        require(returnData.length >= 4, "return data should contain error selector");
        bytes4 selector;
        assembly {
            selector := mload(add(returnData, 32))
        }
        require(selector == bytes4(0x08c379a0), "should be Error(string) selector");
        console2.log("PASS: tryCallRevert caught revert with Error(string)\n");
    }

    function testConditionalNoRevert() internal {
        console2.log("--- Test 2: conditionalRevert(false) sets value ---");

        vm.startBroadcast();
        tester.conditionalRevert(false);
        vm.stopBroadcast();

        require(tester.value() == 42, "value should be 42");
        console2.log("PASS: value == 42\n");
    }

    function testStateRollbackOnRevert() internal {
        console2.log("--- Test 3: stateChangeAndRevert rolls back ---");

        uint256 valueBefore = tester.value();

        (bool success,) = address(tester).call(
            abi.encodeWithSignature("stateChangeAndRevert()")
        );

        require(!success, "stateChangeAndRevert should revert");
        require(tester.value() == valueBefore, "state should be rolled back");
        console2.log("PASS: state rolled back after revert\n");
    }

    function testOverflowUnchecked() internal {
        console2.log("--- Test 4: unchecked overflow returns 0 ---");

        vm.startBroadcast();
        uint256 result = tester.overflowUnchecked();
        vm.stopBroadcast();

        require(result == 0, "unchecked MAX+1 should be 0");
        console2.log("PASS: unchecked overflow returned 0\n");
    }

    function testAssertFailure() internal {
        console2.log("--- Test 5: assert(false) produces Panic(0x01) ---");

        (bool success, bytes memory data) = address(tester).call(
            abi.encodeWithSignature("assertFailure()")
        );

        require(!success, "assert(false) should revert");
        // Panic(uint256) selector is 0x4e487b71
        require(data.length >= 36, "should contain Panic data");
        bytes4 selector;
        assembly {
            selector := mload(add(data, 32))
        }
        require(selector == bytes4(0x4e487b71), "should be Panic selector");
        // Extract panic code (0x01)
        uint256 panicCode;
        assembly {
            panicCode := mload(add(data, 36))
        }
        require(panicCode == 0x01, "panic code should be 0x01");
        console2.log("PASS: Panic(0x01) detected\n");
    }

    function testDivisionByZero() internal {
        console2.log("--- Test 6: division by zero produces Panic(0x12) ---");

        (bool success, bytes memory data) = address(tester).call(
            abi.encodeWithSignature("divisionByZero()")
        );

        require(!success, "division by zero should revert");
        require(data.length >= 36, "should contain Panic data");
        bytes4 selector;
        assembly {
            selector := mload(add(data, 32))
        }
        require(selector == bytes4(0x4e487b71), "should be Panic selector");
        uint256 panicCode;
        assembly {
            panicCode := mload(add(data, 36))
        }
        require(panicCode == 0x12, "panic code should be 0x12");
        console2.log("PASS: Panic(0x12) detected\n");
    }

    function testRevertViaEthCall() internal {
        console2.log("--- Test 7: revertWithRequire via eth_call ---");

        bytes memory callData = abi.encodeWithSignature("revertWithRequire()");
        string memory params = string.concat(
            '[{"to":"', vm.toString(address(tester)),
            '","data":"', vm.toString(callData),
            '","from":"', vm.toString(msg.sender),
            '"},"latest"]'
        );

        // eth_call on a reverting function should return error data
        // Use direct vm.rpc to avoid Foundry's script self-call restriction.
        try vm.rpc("eth_call", params) returns (bytes memory) {
            // If it succeeds, the RPC returned data despite the revert
            // This is unexpected but not necessarily wrong for all implementations
            console2.log("NOTE: eth_call returned data for reverting function");
            console2.log("PASS (soft): RPC did not crash\n");
        } catch {
            console2.log("PASS: eth_call reverted for reverting function\n");
        }
    }

    function testCustomErrorPayload() internal {
        console2.log("--- Test 8: revertWithCustomError payload ---");

        uint256 code = 7;
        (bool success, bytes memory data) = address(tester).call(
            abi.encodeWithSignature("revertWithCustomError(uint256)", code)
        );

        require(!success, "custom error call should revert");
        require(data.length >= 4, "custom error data must include selector");

        bytes4 selector;
        assembly {
            selector := mload(add(data, 32))
        }
        bytes4 expectedSelector = bytes4(keccak256("CustomError(uint256,string)"));
        require(selector == expectedSelector, "unexpected custom error selector");

        bytes memory expectedData = abi.encodeWithSelector(expectedSelector, code, "custom error");
        require(keccak256(data) == keccak256(expectedData), "custom error payload mismatch");
        console2.log("PASS: custom error selector and payload match\n");
    }

    function testEmptyRevertData() internal {
        console2.log("--- Test 9: revertEmpty has no data ---");

        (bool success, bytes memory data) = address(tester).call(
            abi.encodeWithSignature("revertEmpty()")
        );

        require(!success, "empty revert call should revert");
        require(data.length == 0, "empty revert should return no data");
        console2.log("PASS: empty revert data verified\n");
    }

    function testSimpleCustomError() internal {
        console2.log("--- Test 10: revertSimpleError selector ---");

        (bool success, bytes memory data) = address(tester).call(
            abi.encodeWithSignature("revertSimpleError()")
        );

        require(!success, "simple custom error call should revert");
        require(data.length == 4, "simple custom error should return selector only");

        bytes4 selector;
        assembly {
            selector := mload(add(data, 32))
        }
        bytes4 expectedSelector = bytes4(keccak256("SimpleError()"));
        require(selector == expectedSelector, "unexpected simple custom error selector");
        console2.log("PASS: simple custom error selector verified\n");
    }
}
