// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcHelper} from "./RpcHelper.s.sol";
import {CallConsistencyTester} from "../src/CallConsistencyTester.sol";

contract CallConsistencyRead is RpcHelper {
    address constant FUNDED_SENDER = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

    CallConsistencyTester tester;

    function run() public {
        console2.log("=== eth_call vs Execution Consistency Tests (Read Phase) ===\n");

        address target = resolveTargetAddress();
        tester = CallConsistencyTester(target);
        console2.log("CallConsistencyTester target:", target);
        console2.log("");

        require(waitForContractCode(target), "contract code not visible via RPC");

        // Read-only phase: state-changing actions are done in deploy phase to avoid
        // forge script local simulation state diverging from RPC-observed chain state.
        testComputeHash();
        testCounterIsZero();
        testConditionalOnState();
        testBlockDependentValue();

        console2.log("=== Call Consistency Tests Complete ===\n");
    }

    function resolveTargetAddress() internal view returns (address) {
        string memory envAddress = vm.envOr("CALL_CONSISTENCY_TARGET", string(""));
        require(bytes(envAddress).length > 0, "CALL_CONSISTENCY_TARGET is required");
        return vm.parseAddress(envAddress);
    }

    function testComputeHash() internal {
        console2.log("--- Test 1: computeHash via eth_call ---");

        bytes memory data = "hello sovereign";
        bytes32 expected = keccak256(data);

        bytes memory callData = abi.encodeWithSignature("computeHash(bytes)", data);
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = normalizeRpcHexData(vm.rpc("eth_call", params));
        require(rpcResult.length == 32, "eth_call computeHash must return 32 bytes");

        bytes32 rpcHash = abi.decode(rpcResult, (bytes32));
        require(rpcHash == expected, "eth_call hash mismatch");
        console2.log("PASS: eth_call keccak256 matches local\n");
    }

    function testCounterIsZero() internal {
        console2.log("--- Test 2: getCounter via eth_call (expect 0) ---");

        uint256 counterVal = getCounterViaEthCall();
        require(counterVal == 0, "counter should be 0 before increment");
        console2.log("PASS: counter == 0\n");
    }

    function testConditionalOnState() internal {
        console2.log("--- Test 3: conditionalOnState via eth_call ---");

        bytes memory callData = abi.encodeWithSignature("conditionalOnState()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = normalizeRpcHexData(vm.rpc("eth_call", params));

        string memory result = abi.decode(rpcResult, (string));
        require(
            keccak256(bytes(result)) == keccak256(bytes("zero")),
            "conditionalOnState should return 'zero'"
        );
        console2.log("PASS: conditionalOnState returns correct branch\n");
    }

    function testBlockDependentValue() internal {
        console2.log("--- Test 4: getBlockDependentValue via eth_call ---");

        bytes memory callData = abi.encodeWithSignature("getBlockDependentValue()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = normalizeRpcHexData(vm.rpc("eth_call", params));

        (uint256 blockNum, uint256 timestamp, uint256 basefee) =
            abi.decode(rpcResult, (uint256, uint256, uint256));

        console2.log("block.number:", blockNum);
        console2.log("block.timestamp:", timestamp);
        console2.log("block.basefee:", basefee);

        require(blockNum > 0, "block.number must be > 0");
        if (basefee == 0) {
            console2.log("NOTE: block.basefee is zero in this call context");
        }
        console2.log("PASS: block-dependent values are valid\n");
    }

    function buildEthCallParams(address to, bytes memory data) internal view returns (string memory) {
        return string.concat(
            '[{"to":"', vm.toString(to),
            '","data":"', vm.toString(data),
            '","from":"', vm.toString(FUNDED_SENDER),
            '"},"latest"]'
        );
    }

    function getCounterViaEthCall() internal returns (uint256) {
        bytes memory callData = abi.encodeWithSignature("getCounter()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = normalizeRpcHexData(vm.rpc("eth_call", params));
        return abi.decode(rpcResult, (uint256));
    }

    function waitForContractCode(address target) internal returns (bool) {
        for (uint256 i = 0; i < 60; i++) {
            bytes memory latestCode = getCodeAt(target, "latest");
            if (latestCode.length > 0) {
                return true;
            }

            bytes memory pendingCode = getCodeAt(target, "pending");
            if (pendingCode.length > 0) {
                return true;
            }

            vm.sleep(250);
        }
        return false;
    }

    function getCodeAt(address target, string memory blockTag) internal returns (bytes memory) {
        string memory params = string.concat(
            '["', vm.toString(target), '","', blockTag, '"]'
        );
        return normalizeRpcHexData(vm.rpc("eth_getCode", params));
    }

    function normalizeRpcHexData(bytes memory raw) internal returns (bytes memory) {
        if (isHexStringBytes(raw)) {
            return vm.parseBytes(string(raw));
        }

        (bool ok, bytes memory decoded) = tryDecodeAbiEncodedHexString(raw);
        if (ok) {
            return vm.parseBytes(string(decoded));
        }

        return raw;
    }
}
