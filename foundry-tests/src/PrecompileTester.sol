// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract PrecompileTester {
    // Test: Call identity precompile (0x04) and verify success
    function testIdentityPrecompile() public view returns (bool success, bytes memory result) {
        bytes memory input = "Hello, precompile!";

        // Call identity precompile at address 0x04
        (success, result) = address(0x04).staticcall(input);

        // Verify result matches input
        require(success, "Identity precompile call failed");
        require(keccak256(result) == keccak256(input), "Identity precompile returned wrong data");
    }
}
