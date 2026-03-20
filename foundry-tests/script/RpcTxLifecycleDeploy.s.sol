// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {RpcLifecycleTester} from "../src/RpcLifecycleTester.sol";

contract RpcTxLifecycleDeploy is Script {
    function run() public {
        vm.startBroadcast();
        console2.log("=== RPC Tx Lifecycle Compatibility Tests (Deploy Phase) ===\n");

        RpcLifecycleTester tester = new RpcLifecycleTester();
        tester.updateValue(11);
        tester.bumpCounter();

        vm.stopBroadcast();

        console2.log("RpcLifecycleTester deployed at:", address(tester));
        console2.log(string.concat("RPC_LIFECYCLE_TARGET=", vm.toString(address(tester))));
        console2.log("");
    }
}
