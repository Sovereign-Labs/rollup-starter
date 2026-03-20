// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcAssertions} from "./RpcAssertions.sol";
import {RpcLifecycleTester} from "../src/RpcLifecycleTester.sol";

contract RpcTxLifecycleTests is RpcAssertions {
    RpcLifecycleTester tester;
    string txPrimary;
    string txSecondary;

    function run() public {
        console2.log("=== RPC Tx Lifecycle Compatibility Tests (Read Phase) ===\n");

        address target = resolveTargetAddress();
        tester = RpcLifecycleTester(target);
        txPrimary = resolveRequiredEnv("RPC_LIFECYCLE_TX_PRIMARY");
        txSecondary = vm.envOr("RPC_LIFECYCLE_TX_SECONDARY", txPrimary);

        assertHexHashString(txPrimary, "RPC_LIFECYCLE_TX_PRIMARY");
        assertHexHashString(txSecondary, "RPC_LIFECYCLE_TX_SECONDARY");
        require(waitForContractCode(target), "contract code not visible via RPC");

        console2.log("RpcLifecycleTester target:", target);
        console2.log("");

        testMinedTransactionLifecycleConsistency(txPrimary);
        testReceiptPollingTransition(txSecondary);

        console2.log("=== RPC Tx Lifecycle Tests Complete ===\n");
    }

    function resolveTargetAddress() internal view returns (address) {
        string memory envAddress = resolveRequiredEnv("RPC_LIFECYCLE_TARGET");
        return vm.parseAddress(envAddress);
    }

    function resolveRequiredEnv(string memory key) internal view returns (string memory value) {
        value = vm.envOr(key, string(""));
        require(bytes(value).length > 0, string.concat(key, " is required"));
    }

    function testMinedTransactionLifecycleConsistency(string memory txHash) internal {
        console2.log("--- Test 1: tx/receipt/block cross-RPC consistency ---");

        assertHexHashString(txHash, "broadcast.txHash");

        string memory txJson = rpcJsonByFfi("eth_getTransactionByHash", string.concat('["', txHash, '"]'));
        string memory receiptJson = waitForReceiptJsonByFfi(txHash, 80, 250);

        require(!isJsonNull(txJson), "eth_getTransactionByHash returned null");
        require(!isJsonNull(receiptJson), "eth_getTransactionReceipt returned null");

        (string memory blockNumber, string memory blockHash, string memory txIndex) =
            assertTxReceiptConsistency(txJson, receiptJson, txHash);
        assertReceiptFeeFields(receiptJson);
        assertBlockContainment(blockNumber, blockHash, txIndex, txHash);

        console2.log("PASS: tx, receipt, and block linkage is internally consistent\n");
    }

    function assertTxReceiptConsistency(string memory txJson, string memory receiptJson, string memory txHash)
        internal
        returns (string memory blockNumber, string memory blockHash, string memory txIndex)
    {
        string memory txHashFromObj = jsonString(txJson, ".hash", "tx.hash");
        string memory receiptHash = jsonString(receiptJson, ".transactionHash", "receipt.transactionHash");
        assertHexHashString(txHashFromObj, "tx.hash");
        assertHexHashString(receiptHash, "receipt.transactionHash");
        require(keccak256(bytes(txHashFromObj)) == keccak256(bytes(txHash)), "tx.hash mismatch");
        require(keccak256(bytes(receiptHash)) == keccak256(bytes(txHash)), "receipt.transactionHash mismatch");

        string memory txBlockHash = jsonString(txJson, ".blockHash", "tx.blockHash");
        string memory receiptBlockHash = jsonString(receiptJson, ".blockHash", "receipt.blockHash");
        assertHexHashString(txBlockHash, "tx.blockHash");
        assertHexHashString(receiptBlockHash, "receipt.blockHash");
        require(keccak256(bytes(txBlockHash)) == keccak256(bytes(receiptBlockHash)), "tx.blockHash != receipt.blockHash");

        string memory txBlockNumber = jsonString(txJson, ".blockNumber", "tx.blockNumber");
        string memory receiptBlockNumber = jsonString(receiptJson, ".blockNumber", "receipt.blockNumber");
        assertHexQuantityString(txBlockNumber, "tx.blockNumber");
        assertHexQuantityString(receiptBlockNumber, "receipt.blockNumber");
        require(
            keccak256(bytes(txBlockNumber)) == keccak256(bytes(receiptBlockNumber)),
            "tx.blockNumber != receipt.blockNumber"
        );

        string memory txIdx = jsonString(txJson, ".transactionIndex", "tx.transactionIndex");
        string memory receiptIdx = jsonString(receiptJson, ".transactionIndex", "receipt.transactionIndex");
        assertHexQuantityString(txIdx, "tx.transactionIndex");
        assertHexQuantityString(receiptIdx, "receipt.transactionIndex");
        require(keccak256(bytes(txIdx)) == keccak256(bytes(receiptIdx)), "tx.index != receipt.index");

        return (receiptBlockNumber, receiptBlockHash, receiptIdx);
    }

    function assertReceiptFeeFields(string memory receiptJson) internal {
        assertHexQuantityString(
            jsonString(receiptJson, ".gasUsed", "receipt.gasUsed"), "receipt.gasUsed"
        );
        assertHexQuantityString(
            jsonString(receiptJson, ".cumulativeGasUsed", "receipt.cumulativeGasUsed"), "receipt.cumulativeGasUsed"
        );
        assertHexQuantityString(
            jsonString(receiptJson, ".effectiveGasPrice", "receipt.effectiveGasPrice"), "receipt.effectiveGasPrice"
        );
        assertHexDataString(
            jsonString(receiptJson, ".logsBloom", "receipt.logsBloom"), "receipt.logsBloom"
        );
    }

    function assertBlockContainment(
        string memory blockNumber,
        string memory blockHash,
        string memory txIndex,
        string memory txHash
    ) internal returns (bool) {
        string memory blockByNumberHashesJson = rpcJsonByFfi("eth_getBlockByNumber", string.concat('["', blockNumber, '",false]'));
        string memory blockByNumberFullJson = rpcJsonByFfi("eth_getBlockByNumber", string.concat('["', blockNumber, '",true]'));
        string memory blockByHashJson = rpcJsonByFfi("eth_getBlockByHash", string.concat('["', blockHash, '",false]'));

        string[] memory txHashes = vm.parseJsonStringArray(blockByNumberHashesJson, ".transactions");
        require(containsString(txHashes, txHash), "blockByNumber(false) missing tx hash");

        uint256 txIndexU256 = parseHexQuantityString(txIndex);
        string memory fullTxPath = string.concat(".transactions[", vm.toString(txIndexU256), "].hash");
        string memory fullTxHash = jsonString(blockByNumberFullJson, fullTxPath, "blockByNumber(true).transactions[idx].hash");
        assertHexHashString(fullTxHash, "blockByNumber(true).transactions[idx].hash");
        require(keccak256(bytes(fullTxHash)) == keccak256(bytes(txHash)), "blockByNumber(true) hash mismatch");

        string[] memory txHashesByHash = vm.parseJsonStringArray(blockByHashJson, ".transactions");
        require(containsString(txHashesByHash, txHash), "blockByHash(false) missing tx hash");
        return true;
    }

    function testReceiptPollingTransition(string memory txHash) internal {
        console2.log("--- Test 2: receipt polling transition is stable ---");

        string memory params = string.concat('["', txHash, '"]');

        string memory immediateReceipt = rpcJsonByFfi("eth_getTransactionReceipt", params);
        string memory finalReceipt = immediateReceipt;
        if (isJsonNull(immediateReceipt)) {
            finalReceipt = waitForReceiptJsonByFfi(txHash, 80, 250);
        }

        require(!isJsonNull(finalReceipt), "receipt remained null after polling");
        assertHexHashString(
            jsonString(finalReceipt, ".transactionHash", "polledReceipt.transactionHash"), "polledReceipt.transactionHash"
        );
        assertHexQuantityString(jsonString(finalReceipt, ".status", "polledReceipt.status"), "polledReceipt.status");
        console2.log("PASS: receipt polling returns stable decode-safe object\n");
    }

    function containsString(string[] memory values, string memory needle) internal pure returns (bool) {
        for (uint256 i = 0; i < values.length; i++) {
            if (keccak256(bytes(values[i])) == keccak256(bytes(needle))) {
                return true;
            }
        }
        return false;
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
        string memory params = string.concat('["', vm.toString(target), '","', blockTag, '"]');
        return normalizeRpcHexData(rpcResult("eth_getCode", params));
    }

    function waitForReceiptJsonByFfi(string memory txHash, uint256 attempts, uint256 sleepMs)
        internal
        returns (string memory)
    {
        string memory params = string.concat('["', txHash, '"]');
        for (uint256 i = 0; i < attempts; i++) {
            string memory receiptJson = rpcJsonByFfi("eth_getTransactionReceipt", params);
            if (!isJsonNull(receiptJson)) {
                return receiptJson;
            }
            vm.sleep(sleepMs);
        }
        revert("timed out waiting for transaction receipt");
    }
}
