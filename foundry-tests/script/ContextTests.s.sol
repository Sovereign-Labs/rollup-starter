// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {ContextTester} from "../src/ContextTester.sol";

contract ContextTests is Script {
    function run() public {
        vm.startBroadcast();

        console2.log("=== Phase 7: Block/Transaction Context Tests ===\n");

        ContextTester tester = new ContextTester();
        console2.log("ContextTester deployed at:", address(tester));
        console2.log("");

        testBlockContext(tester);
        testTxContext(tester);

        vm.stopBroadcast();
    }

    function testBlockContext(ContextTester tester) internal view {
        console2.log("--- Test 1: Block Context ---");

        (
            uint256 blockNumber,
            uint256 blockTimestamp,
            uint256 blockGaslimit,
            uint256 blockChainid,
            uint256 blockBasefee
        ) = tester.getBlockContext();

        console2.log("block.number:", blockNumber);
        console2.log("block.timestamp:", blockTimestamp);
        console2.log("block.gaslimit:", blockGaslimit);
        console2.log("block.chainid:", blockChainid);
        console2.log("block.basefee:", blockBasefee);

        require(blockTimestamp > 0, "block.timestamp should not be zero");
        require(blockGaslimit > 0, "block.gaslimit should not be zero");
        require(blockChainid > 0, "block.chainid should not be zero");

        console2.log("");
    }

    function testTxContext(ContextTester tester) internal view {
        console2.log("--- Test 2: Transaction Context ---");

        (
            address msgSender,
            address txOrigin,
            uint256 txGasprice
        ) = tester.getTxContext();

        console2.log("msg.sender:", msgSender);
        console2.log("tx.origin:", txOrigin);
        console2.log("tx.gasprice:", txGasprice);

        require(txOrigin == msgSender, "tx.origin should equal msg.sender");

        console2.log("");
    }
}
