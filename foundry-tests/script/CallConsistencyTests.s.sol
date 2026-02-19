// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {CallConsistencyTester} from "../src/CallConsistencyTester.sol";

contract CallConsistencyTests is Script {
    CallConsistencyTester tester;

    function run() public {
        vm.startBroadcast();
        console2.log("=== eth_call vs Execution Consistency Tests ===\n");

        tester = new CallConsistencyTester();
        console2.log("CallConsistencyTester deployed at:", address(tester));
        console2.log("");
        vm.stopBroadcast();

        testComputeHash();
        testCounterBeforeIncrement();
        testCounterAfterIncrement();
        testConditionalOnState();
        testBlockDependentValue();

        console2.log("=== Call Consistency Tests Complete ===\n");
    }

    function testComputeHash() internal {
        console2.log("--- Test 1: computeHash via eth_call ---");

        bytes memory data = "hello sovereign";
        bytes32 expected = keccak256(data);

        bytes memory callData = abi.encodeWithSignature("computeHash(bytes)", data);
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = vm.rpc("eth_call", params);

        bytes32 rpcHash = abi.decode(rpcResult, (bytes32));
        require(rpcHash == expected, "eth_call hash mismatch");
        console2.log("PASS: eth_call keccak256 matches local\n");
    }

    function testCounterBeforeIncrement() internal {
        console2.log("--- Test 2: getCounter via eth_call (expect 0) ---");

        bytes memory callData = abi.encodeWithSignature("getCounter()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = vm.rpc("eth_call", params);

        uint256 counterVal = abi.decode(rpcResult, (uint256));
        require(counterVal == 0, "counter should be 0 before increment");
        console2.log("PASS: counter == 0\n");
    }

    function testCounterAfterIncrement() internal {
        console2.log("--- Test 3: incrementAndReturn then getCounter ---");

        vm.startBroadcast();
        uint256 returned = tester.incrementAndReturn();
        vm.stopBroadcast();
        require(returned == 1, "incrementAndReturn should return 1");

        bytes memory callData = abi.encodeWithSignature("getCounter()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = vm.rpc("eth_call", params);

        uint256 counterVal = abi.decode(rpcResult, (uint256));
        require(counterVal == 1, "counter should be 1 after increment");
        console2.log("PASS: counter == 1 after increment\n");
    }

    function testConditionalOnState() internal {
        console2.log("--- Test 4: conditionalOnState via eth_call ---");

        bytes memory callData = abi.encodeWithSignature("conditionalOnState()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = vm.rpc("eth_call", params);

        string memory result = abi.decode(rpcResult, (string));
        // counter is 1 after test 3
        require(
            keccak256(bytes(result)) == keccak256(bytes("one")),
            "conditionalOnState should return 'one'"
        );
        console2.log("PASS: conditionalOnState returns correct branch\n");
    }

    function testBlockDependentValue() internal {
        console2.log("--- Test 5: getBlockDependentValue via eth_call ---");

        bytes memory callData = abi.encodeWithSignature("getBlockDependentValue()");
        string memory params = buildEthCallParams(address(tester), callData);
        bytes memory rpcResult = vm.rpc("eth_call", params);

        (uint256 blockNum, uint256 timestamp, uint256 basefee) =
            abi.decode(rpcResult, (uint256, uint256, uint256));

        console2.log("block.number:", blockNum);
        console2.log("block.timestamp:", timestamp);
        console2.log("block.basefee:", basefee);

        require(blockNum > 0, "block.number must be > 0");
        require(basefee > 0, "block.basefee must be > 0");
        console2.log("PASS: block-dependent values are valid\n");
    }

    function buildEthCallParams(address to, bytes memory data) internal view returns (string memory) {
        return string.concat(
            '[{"to":"', vm.toString(to),
            '","data":"', vm.toString(data),
            '","from":"', vm.toString(msg.sender),
            '"},"latest"]'
        );
    }
}
