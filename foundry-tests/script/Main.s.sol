// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Main} from "../src/Main.sol";

contract MainScript is Script {
    Main public main;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        main = new Main();

        vm.stopBroadcast();
    }
}
