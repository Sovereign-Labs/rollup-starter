// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";

contract DebugVmRpc is Script {
    function run() public {
        bytes memory bn = vm.rpc("eth_blockNumber", "[]");
        console2.log("eth_blockNumber len", bn.length);
        console2.logBytes(bn);

        bytes memory blockObj = vm.rpc("eth_getBlockByNumber", '["latest", false]');
        console2.log("eth_getBlockByNumber len", blockObj.length);
        console2.logBytes(blockObj);

        bytes memory txObj = vm.rpc(
            "eth_getTransactionByHash",
            '["0x1111111111111111111111111111111111111111111111111111111111111111"]'
        );
        console2.log("eth_getTransactionByHash len", txObj.length);
        console2.logBytes(txObj);

        bytes memory logsObj = vm.rpc(
            "eth_getLogs",
            '[{"fromBlock":"0x0","toBlock":"latest","address":"0x0000000000000000000000000000000000000001"}]'
        );
        console2.log("eth_getLogs len", logsObj.length);
        console2.logBytes(logsObj);
    }
}
