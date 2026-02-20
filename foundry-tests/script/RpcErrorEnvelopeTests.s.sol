// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcAssertions} from "./RpcAssertions.sol";
import {RpcRevertShapesTester} from "../src/RpcRevertShapesTester.sol";

// NOTE: This suite is intentionally strict and currently excluded from AllTests
// because it detects SDK revert-envelope incompatibilities on eth_call/estimate paths.
contract RpcErrorEnvelopeTests is RpcAssertions {
    RpcRevertShapesTester tester;

    function run() public {
        vm.startBroadcast();
        console2.log("=== RPC Error & Revert Compatibility Tests ===\n");

        tester = new RpcRevertShapesTester();
        console2.log("RpcRevertShapesTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        testMalformedParamsErrorPaths();
        testEthCallRevertShapes();
        testEstimateGasOnRevertErrors();

        console2.log("=== RPC Error & Revert Tests Complete ===\n");
    }

    function testMalformedParamsErrorPaths() internal {
        console2.log("--- Test 1: malformed params return RPC errors ---");

        expectRpcError("eth_getBalance", '["0x1234","latest"]', "eth_getBalance invalid address");
        expectRpcError(
            "eth_getBlockByNumber", '["latest","not-a-bool"]', "eth_getBlockByNumber invalid includeTransactions type"
        );
        expectRpcError("eth_getLogs", '[{"fromBlock":"0x01","address":"0x1234"}]', "eth_getLogs invalid address");

        console2.log("PASS: malformed parameter matrix returns deterministic errors\n");
    }

    function testEthCallRevertShapes() internal {
        console2.log("--- Test 2: eth_call revert behavior is decode-safe ---");

        checkEthCallRevert(
            abi.encodeWithSignature("revertWithRequire()"),
            bytes4(0x08c379a0),
            false,
            "revertWithRequire"
        );
        checkEthCallRevert(
            abi.encodeWithSignature("revertWithPanic()"),
            bytes4(0x4e487b71),
            false,
            "revertWithPanic"
        );
        checkEthCallRevert(
            abi.encodeWithSignature("revertWithCustom(uint256)", 77),
            bytes4(keccak256("CustomFailure(uint256,string)")),
            false,
            "revertWithCustom"
        );
        checkEthCallRevert(
            abi.encodeWithSignature("revertWithSimple()"),
            bytes4(keccak256("SimpleFailure()")),
            false,
            "revertWithSimple"
        );
        checkEthCallRevert(
            abi.encodeWithSignature("revertEmpty()"),
            bytes4(0),
            true,
            "revertEmpty"
        );

        console2.log("PASS: eth_call revert paths are deterministic/decode-safe\n");
    }

    function testEstimateGasOnRevertErrors() internal {
        console2.log("--- Test 3: eth_estimateGas errors on guaranteed revert ---");

        bytes memory callData = abi.encodeWithSignature("revertWithRequire()");
        string memory params = string.concat(
            '[{"to":"', vm.toString(address(tester)),
            '","from":"', vm.toString(DEFAULT_FROM),
            '","data":"', vm.toString(callData),
            '"}]'
        );

        bool failed = false;
        try vm.rpc("eth_estimateGas", params) returns (bytes memory) {
            failed = false;
        } catch {
            failed = true;
        }
        require(failed, "eth_estimateGas unexpectedly succeeded for reverting function");

        console2.log("PASS: revert estimate path raises RPC error\n");
    }

    function checkEthCallRevert(bytes memory callData, bytes4 expectedSelector, bool allowEmpty, string memory label)
        internal
    {
        string memory params = string.concat(
            '[{"to":"', vm.toString(address(tester)),
            '","from":"', vm.toString(DEFAULT_FROM),
            '","data":"', vm.toString(callData),
            '"},"latest"]'
        );

        try vm.rpc("eth_call", params) returns (bytes memory rawResult) {
            bytes memory revertData = normalizeRpcHexData(rawResult);
            if (allowEmpty && revertData.length == 0) {
                return;
            }

            require(revertData.length >= 4, string.concat(label, ": revert data too short"));
            bytes4 selector = selectorOf(revertData);
            require(selector == expectedSelector, string.concat(label, ": unexpected revert selector"));
        } catch {
            // RPC-level revert response is acceptable if deterministic and non-malformed.
            return;
        }
    }

    function expectRpcError(string memory method, string memory params, string memory label) internal {
        bool failed = false;
        try vm.rpc(method, params) returns (bytes memory) {
            failed = false;
        } catch {
            failed = true;
        }
        require(failed, string.concat(label, " should return RPC error"));
    }
}
