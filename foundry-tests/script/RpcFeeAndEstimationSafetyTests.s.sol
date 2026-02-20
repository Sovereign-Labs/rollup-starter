// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcAssertions} from "./RpcAssertions.sol";
import {GasEstimationTester} from "../src/GasEstimationTester.sol";

// NOTE: This suite is intentionally strict and currently excluded from AllTests
// because it detects SDK fee endpoint incompatibilities (e.g. maxPriorityFee=0x00).
contract RpcFeeAndEstimationSafetyTests is RpcAssertions {
    GasEstimationTester tester;
    address broadcaster;

    function run() public {
        vm.startBroadcast();
        console2.log("=== RPC Fee & Estimation Safety Tests ===\n");
        broadcaster = tx.origin;

        tester = new GasEstimationTester();
        console2.log("GasEstimationTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        testFeeEndpointsShape();

        if (ethBalance(broadcaster) == 0) {
            console2.log("SKIP: broadcaster has zero ETH; estimation execution linkage requires funded sender");
            console2.log("");
            return;
        }

        testEstimateExecutionLinkage();
        testEstimateOnRevertPathErrors();

        console2.log("=== RPC Fee & Estimation Safety Tests Complete ===\n");
    }

    function testFeeEndpointsShape() internal {
        console2.log("--- Test 1: fee endpoints are decode-safe ---");

        uint256 gasPrice = decodeRpcQuantity(rpcResult("eth_gasPrice", "[]"));
        require(gasPrice > 0, "eth_gasPrice returned zero");

        uint256 maxPriorityFee = decodeRpcQuantity(rpcResult("eth_maxPriorityFeePerGas", "[]"));
        require(maxPriorityFee > 0, "eth_maxPriorityFeePerGas returned zero");

        string memory feeHistoryJson = rpcJson("eth_feeHistory", '["0x3","latest",[]]');
        string memory oldestBlock = jsonString(feeHistoryJson, ".oldestBlock", "feeHistory.oldestBlock");
        assertHexQuantityString(oldestBlock, "feeHistory.oldestBlock");

        string[] memory baseFees = vm.parseJsonStringArray(feeHistoryJson, ".baseFeePerGas");
        require(baseFees.length == 4, "feeHistory.baseFeePerGas length must be blockCount+1");
        for (uint256 i = 0; i < baseFees.length; i++) {
            assertHexQuantityString(baseFees[i], "feeHistory.baseFeePerGas[i]");
        }

        require(jsonPathExists(feeHistoryJson, ".gasUsedRatio"), "feeHistory.gasUsedRatio missing");
        require(jsonPathExists(feeHistoryJson, ".reward"), "feeHistory.reward missing");
        console2.log("PASS: eth_gasPrice / eth_maxPriorityFeePerGas / eth_feeHistory are decode-safe\n");
    }

    function testEstimateExecutionLinkage() internal {
        console2.log("--- Test 2: estimate/execution/effectiveGasPrice coherence ---");

        bytes memory callData = abi.encodeWithSignature("setValue(uint256)", 42);
        string memory txObj = buildTxJson(address(tester), callData);

        uint256 estimate = decodeRpcQuantity(rpcResult("eth_estimateGas", txObj));
        require(estimate > 0, "estimate must be > 0");

        uint256 fromBlock = currentBlockNumber();
        vm.startBroadcast();
        tester.setValue(42);
        vm.stopBroadcast();

        string memory txHash = waitForValueSetTxHash(42, fromBlock, 80, 250);

        string memory receiptJson = waitForReceiptJson(txHash, 80, 250);
        string memory txJson = rpcJson("eth_getTransactionByHash", string.concat('["', txHash, '"]'));

        string memory gasUsedHex = jsonString(receiptJson, ".gasUsed", "receipt.gasUsed");
        assertHexQuantityString(gasUsedHex, "receipt.gasUsed");
        uint256 gasUsed = parseHexQuantityString(gasUsedHex);
        require(estimate >= gasUsed, "estimateGas < actual gasUsed");

        // High-signal bound: extreme overestimation breaks wallet UX and fee previews.
        require(estimate <= gasUsed * 4, "estimateGas is >4x actual gasUsed");

        string memory txGasPriceHex = jsonString(txJson, ".gasPrice", "tx.gasPrice");
        string memory effectiveGasPriceHex =
            jsonString(receiptJson, ".effectiveGasPrice", "receipt.effectiveGasPrice");
        assertHexQuantityString(txGasPriceHex, "tx.gasPrice");
        assertHexQuantityString(effectiveGasPriceHex, "receipt.effectiveGasPrice");
        require(
            parseHexQuantityString(txGasPriceHex) == parseHexQuantityString(effectiveGasPriceHex),
            "tx.gasPrice != receipt.effectiveGasPrice"
        );

        console2.log("Estimate:", estimate);
        console2.log("Actual gasUsed:", gasUsed);
        console2.log("PASS: estimate/execution/receipt fee fields are coherent\n");
    }

    function testEstimateOnRevertPathErrors() internal {
        console2.log("--- Test 3: eth_estimateGas must error on reverting call ---");

        bytes memory callData = abi.encodeWithSignature("revertIf(bool)", true);
        string memory txObj = buildTxJson(address(tester), callData);

        bool reverted = false;
        try this.doRpc("eth_estimateGas", txObj) returns (bytes memory) {
            reverted = false;
        } catch {
            reverted = true;
        }

        require(reverted, "eth_estimateGas unexpectedly succeeded for reverting call");
        console2.log("PASS: reverting estimate path returns RPC error\n");
    }

    function doRpc(string memory method, string memory params) external returns (bytes memory) {
        return vm.rpc(method, params);
    }

    function buildTxJson(address to, bytes memory data) internal view returns (string memory) {
        return string.concat(
            '[{"to":"', vm.toString(to),
            '","data":"', vm.toString(data),
            '","from":"', vm.toString(broadcaster),
            '"}]'
        );
    }

    function waitForValueSetTxHash(uint256 newValue, uint256 fromBlock, uint256 attempts, uint256 sleepMs)
        internal
        returns (string memory)
    {
        bytes32 topic0 = keccak256("ValueSet(uint256)");
        bytes32 topic1 = bytes32(newValue);

        string memory params = string.concat(
            '[{"fromBlock":"', toHexQuantity(fromBlock),
            '","toBlock":"latest"',
            ',"address":"', vm.toString(address(tester)),
            '","topics":["', vm.toString(topic0), '","', vm.toString(topic1), '"]}]'
        );

        for (uint256 i = 0; i < attempts; i++) {
            string memory logsJson = rpcJson("eth_getLogs", params);
            string memory wrapped = string.concat('{"logs":', logsJson, "}");
            string[] memory keys = jsonKeys(wrapped, ".logs", "ValueSet tx-hash lookup");
            if (keys.length > 0) {
                string memory path = string.concat(".logs[", keys[keys.length - 1], "].transactionHash");
                return jsonString(wrapped, path, "ValueSet transactionHash");
            }
            vm.sleep(sleepMs);
        }

        revert("timed out waiting for ValueSet log");
    }
}
