// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CallConsistencyTester} from "../src/CallConsistencyTester.sol";

contract CallConsistencyDeploy is Script {
    function run() public {
        vm.startBroadcast();
        console2.log("=== eth_call vs Execution Consistency Tests (Deploy Phase) ===\n");

        CallConsistencyTester tester = new CallConsistencyTester();

        vm.stopBroadcast();

        console2.log("CallConsistencyTester deployed at:", address(tester));
        console2.log("");
    }
}
