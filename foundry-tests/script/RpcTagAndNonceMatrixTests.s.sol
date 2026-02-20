// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {console2} from "forge-std/console2.sol";
import {RpcAssertions} from "./RpcAssertions.sol";
import {RpcLifecycleTester} from "../src/RpcLifecycleTester.sol";

// NOTE: This suite is intentionally strict and currently excluded from AllTests
// because it detects SDK nonce progression/tag-coherence incompatibilities.
contract RpcTagAndNonceMatrixTests is RpcAssertions {
    RpcLifecycleTester tester;
    address broadcaster;

    function run() public {
        vm.startBroadcast();
        console2.log("=== RPC Tag & Nonce Matrix Tests ===\n");
        broadcaster = tx.origin;

        tester = new RpcLifecycleTester();
        console2.log("RpcLifecycleTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        testTagReadMatrix();

        if (ethBalance(broadcaster) == 0) {
            console2.log("SKIP: broadcaster has zero ETH; nonce progression send-path requires funded sender");
            console2.log("");
            return;
        }

        testNonceProgressionAfterSends();

        console2.log("=== RPC Tag & Nonce Matrix Tests Complete ===\n");
    }

    function testTagReadMatrix() internal {
        console2.log("--- Test 1: tag matrix for block + nonce endpoints ---");

        string[5] memory tags = ["latest", "pending", "earliest", "safe", "finalized"];
        for (uint256 i = 0; i < tags.length; i++) {
            uint256 nonce = nonceAtTag(tags[i]);
            console2.log("nonce(", tags[i], "):", nonce);

            // Use FFI-backed RPC JSON for object responses because vm.rpc can return
            // non-JSON encoding for object payloads in script mode.
            string memory blockJson = rpcJsonByFfi(
                "eth_getBlockByNumber",
                string.concat('["', tags[i], '",false]')
            );
            if (!isJsonNull(blockJson)) {
                assertHexQuantityString(
                    jsonString(blockJson, ".number", string.concat("block.number@", tags[i])),
                    string.concat("block.number@", tags[i])
                );
                assertHexHashString(
                    jsonString(blockJson, ".hash", string.concat("block.hash@", tags[i])),
                    string.concat("block.hash@", tags[i])
                );
            }
        }

        console2.log("PASS: tag matrix calls are decode-safe and stable\n");
    }

    function testNonceProgressionAfterSends() internal {
        console2.log("--- Test 2: nonce progression pending/latest coherence ---");

        uint256 latestBefore = nonceAtTag("latest");
        uint256 pendingBefore = nonceAtTag("pending");
        require(pendingBefore >= latestBefore, "pending nonce < latest nonce before sends");

        vm.startBroadcast();
        tester.bumpCounter();
        tester.bumpCounter();
        vm.stopBroadcast();

        uint256 latestAfter = nonceAtTag("latest");
        uint256 pendingAfter = nonceAtTag("pending");

        require(latestAfter >= latestBefore + 2, "latest nonce did not progress by two sent txs");
        require(pendingAfter >= latestAfter, "pending nonce < latest nonce after sends");

        console2.log("latest before:", latestBefore);
        console2.log("latest after:", latestAfter);
        console2.log("pending after:", pendingAfter);
        console2.log("PASS: nonce progression and pending/latest ordering are coherent\n");
    }

    function nonceAtTag(string memory tag) internal returns (uint256) {
        string memory params = string.concat(
            '["', vm.toString(broadcaster), '","', tag, '"]'
        );
        return decodeRpcQuantity(rpcResult("eth_getTransactionCount", params));
    }
}
