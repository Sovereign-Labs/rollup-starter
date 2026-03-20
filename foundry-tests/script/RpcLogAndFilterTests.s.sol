// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcAssertions} from "./RpcAssertions.sol";
import {RpcLifecycleTester} from "../src/RpcLifecycleTester.sol";

// NOTE: This suite is intentionally strict and currently excluded from AllTests
// because it detects SDK eth_getLogs response-shape incompatibilities.
contract RpcLogAndFilterTests is RpcAssertions {
    RpcLifecycleTester tester;
    address broadcaster;

    function run() public {
        vm.startBroadcast();
        console2.log("=== RPC Log & Filter Compatibility Tests ===\n");
        broadcaster = tx.origin;

        tester = new RpcLifecycleTester();
        console2.log("RpcLifecycleTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        if (ethBalance(broadcaster) == 0) {
            console2.log("SKIP: broadcaster has zero ETH; log filter checks require emitted txs");
            console2.log("");
            return;
        }

        testAddressAndRangeFilter();
        testTopicWildcardFilter();

        console2.log("=== RPC Log & Filter Tests Complete ===\n");
    }

    function testAddressAndRangeFilter() internal {
        console2.log("--- Test 1: eth_getLogs address/range linkage ---");

        uint256 fromBlockU = currentBlockNumber();
        vm.startBroadcast();
        tester.emitTopicEvent(1, "alpha");
        tester.emitTopicEvent(2, "beta");
        vm.stopBroadcast();
        uint256 toBlockU = currentBlockNumber();

        string memory params = string.concat(
            '[{"fromBlock":"', toHexQuantity(fromBlockU),
            '","toBlock":"', toHexQuantity(toBlockU),
            '","address":"', vm.toString(address(tester)),
            '"}]'
        );
        string memory logsJson = rpcJson("eth_getLogs", params);
        string memory wrapped = string.concat('{"logs":', logsJson, "}");

        string[] memory keys = jsonKeys(wrapped, ".logs", "eth_getLogs result");
        require(keys.length >= 2, "expected at least two logs for address/range filter");

        bool foundIdOne = false;
        bool foundIdTwo = false;
        bytes32 topic0 = keccak256("TopicEvent(address,uint256,string)");
        string memory idOneTopic = vm.toString(bytes32(uint256(1)));
        string memory idTwoTopic = vm.toString(bytes32(uint256(2)));

        for (uint256 i = 0; i < keys.length; i++) {
            string memory base = string.concat(".logs[", keys[i], "]");
            string memory txHash = jsonString(wrapped, string.concat(base, ".transactionHash"), "log.transactionHash");
            assertHexHashString(txHash, "log.transactionHash");

            assertHexHashString(jsonString(wrapped, string.concat(base, ".blockHash"), "log.blockHash"), "log.blockHash");
            assertHexQuantityString(jsonString(wrapped, string.concat(base, ".blockNumber"), "log.blockNumber"), "log.blockNumber");
            assertHexQuantityString(
                jsonString(wrapped, string.concat(base, ".transactionIndex"), "log.transactionIndex"), "log.transactionIndex"
            );
            assertHexQuantityString(jsonString(wrapped, string.concat(base, ".logIndex"), "log.logIndex"), "log.logIndex");
            assertHexDataString(jsonString(wrapped, string.concat(base, ".data"), "log.data"), "log.data");
            require(!jsonBool(wrapped, string.concat(base, ".removed"), "log.removed"), "log.removed should be false");

            string memory logAddress = jsonString(wrapped, string.concat(base, ".address"), "log.address");
            assertHexAddressString(logAddress, "log.address");
            require(vm.parseAddress(logAddress) == address(tester), "log.address mismatch");

            string[] memory topics = vm.parseJsonStringArray(wrapped, string.concat(base, ".topics"));
            require(topics.length >= 3, "expected TopicEvent logs with 3 topics");
            require(
                keccak256(bytes(topics[0])) == keccak256(bytes(vm.toString(topic0))),
                "topic0 mismatch for TopicEvent signature"
            );
            if (keccak256(bytes(topics[2])) == keccak256(bytes(idOneTopic))) {
                foundIdOne = true;
            }
            if (keccak256(bytes(topics[2])) == keccak256(bytes(idTwoTopic))) {
                foundIdTwo = true;
            }
        }

        require(foundIdOne, "address/range filter did not include id=1 event");
        require(foundIdTwo, "address/range filter did not include id=2 event");
        console2.log("PASS: address/range filter returns linked decode-safe logs\n");
    }

    function testTopicWildcardFilter() internal {
        console2.log("--- Test 2: eth_getLogs topic wildcard semantics ---");

        uint256 fromBlockU = currentBlockNumber();
        vm.startBroadcast();
        tester.emitTopicEvent(7, "gamma");
        tester.emitTopicEvent(9, "delta");
        vm.stopBroadcast();
        uint256 toBlockU = currentBlockNumber();

        bytes32 topic0 = keccak256("TopicEvent(address,uint256,string)");
        bytes32 idNineTopic = bytes32(uint256(9));
        string memory idNineTopicHex = vm.toString(idNineTopic);

        string memory params = string.concat(
            '[{"fromBlock":"', toHexQuantity(fromBlockU),
            '","toBlock":"', toHexQuantity(toBlockU),
            '","address":"', vm.toString(address(tester)),
            '","topics":["', vm.toString(topic0), '",null,"', idNineTopicHex, '"]}]'
        );

        assertTopicFilterResults(params, idNineTopicHex);
        console2.log("PASS: wildcard topic filter preserves expected event selection\n");
    }

    function assertTopicFilterResults(
        string memory params,
        string memory idNineTopicHex
    ) internal {
        string memory logsJson = rpcJson("eth_getLogs", params);
        string memory wrapped = string.concat('{"logs":', logsJson, "}");
        string[] memory keys = jsonKeys(wrapped, ".logs", "topic-filter eth_getLogs result");
        require(keys.length >= 1, "topic filter returned no logs");

        bool foundNine = false;
        for (uint256 i = 0; i < keys.length; i++) {
            string memory base = string.concat(".logs[", keys[i], "]");
            string memory txHash = jsonString(wrapped, string.concat(base, ".transactionHash"), "topic-filter log.transactionHash");
            assertHexHashString(txHash, "topic-filter log.transactionHash");

            string[] memory topics = vm.parseJsonStringArray(wrapped, string.concat(base, ".topics"));
            require(topics.length >= 3, "topic-filter log topics length mismatch");
            require(
                keccak256(bytes(topics[2])) == keccak256(bytes(idNineTopicHex)),
                "topic-filter returned unexpected indexed id"
            );
            foundNine = true;
        }

        require(foundNine, "topic wildcard filter missed expected id=9 event");
    }
}
