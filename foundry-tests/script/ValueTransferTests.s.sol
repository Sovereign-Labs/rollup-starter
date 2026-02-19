// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {ValueTransferTester, ValueRejecter, ValueReceiver} from "../src/ValueTransferTester.sol";

contract ValueTransferTests is Script {
    address constant BROADCAST_SENDER = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    uint256 constant MIN_REQUIRED_WEI = 1600;

    ValueTransferTester tester;
    ValueRejecter rejecter;
    ValueReceiver receiver;

    function run() public {
        vm.startBroadcast();
        console2.log("=== Value (ETH) Transfer Tests ===\n");

        tester = new ValueTransferTester();
        rejecter = new ValueRejecter();
        receiver = new ValueReceiver();

        console2.log("ValueTransferTester deployed at:", address(tester));
        console2.log("ValueRejecter deployed at:", address(rejecter));
        console2.log("ValueReceiver deployed at:", address(receiver));
        console2.log("");
        vm.stopBroadcast();

        uint256 senderBalance = getBalance(BROADCAST_SENDER);
        if (senderBalance >= MIN_REQUIRED_WEI) {
            testDirectSend();
            testForwardValue();
            testRejectValue();
        } else {
            console2.log(
                "SKIP: sender has insufficient ETH for positive-value sends (balance:",
                senderBalance,
                "wei)"
            );
            console2.log("");
        }

        testZeroValue();
        testGetBalanceRpc();

        console2.log("=== Value Transfer Tests Complete ===\n");
    }

    function testDirectSend() internal {
        console2.log("--- Test 1: Direct ETH send to contract ---");

        vm.startBroadcast();
        (bool success,) = address(tester).call{value: 1000}("");
        vm.stopBroadcast();

        require(success, "direct send failed");
        require(tester.totalReceived() == 1000, "totalReceived should be 1000");
        require(tester.getBalance() == 1000, "balance should be 1000");
        console2.log("PASS: received 1000 wei\n");
    }

    function testForwardValue() internal {
        console2.log("--- Test 2: Forward ETH through contract ---");

        vm.startBroadcast();
        tester.forwardTo{value: 500}(payable(address(receiver)));
        vm.stopBroadcast();

        require(receiver.getBalance() == 500, "receiver should have 500");
        console2.log("PASS: receiver got 500 wei\n");
    }

    function testRejectValue() internal {
        console2.log("--- Test 3: Send ETH to rejecting contract ---");

        vm.startBroadcast();
        (bool success,) = address(rejecter).call{value: 100}("");
        vm.stopBroadcast();

        require(!success, "send to rejecter should fail");
        console2.log("PASS: rejecter correctly rejected ETH\n");
    }

    function testZeroValue() internal {
        console2.log("--- Test 4: Zero-value send ---");

        vm.startBroadcast();
        (bool success,) = address(tester).call{value: 0}("");
        vm.stopBroadcast();

        require(success, "zero-value send should succeed");
        console2.log("PASS: zero-value send succeeded\n");
    }

    function testGetBalanceRpc() internal {
        console2.log("--- Test 5: eth_getBalance matches address.balance ---");

        uint256 localBalance = address(tester).balance;
        console2.log("Local balance:", localBalance);

        string memory params = string.concat(
            '["', vm.toString(address(tester)), '","latest"]'
        );
        bytes memory rpcResult = vm.rpc("eth_getBalance", params);
        uint256 rpcBalance = decodeRpcQuantity(rpcResult);
        console2.log("RPC balance:", rpcBalance);

        require(rpcBalance == localBalance, "eth_getBalance mismatch with address.balance");
        console2.log("PASS: balances match\n");
    }

    function getBalance(address account) internal returns (uint256) {
        string memory params = string.concat(
            '["', vm.toString(account), '","latest"]'
        );
        bytes memory rpcResult = vm.rpc("eth_getBalance", params);
        return decodeRpcQuantity(rpcResult);
    }

    function decodeRpcQuantity(bytes memory raw) internal pure returns (uint256 value) {
        require(raw.length > 0, "rpc quantity must not be empty");
        require(raw.length <= 32, "rpc quantity too large");
        for (uint256 i = 0; i < raw.length; i++) {
            value = (value << 8) | uint8(raw[i]);
        }
    }
}
