// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract PrecompileTester {
    function testIdentityPrecompile() public view returns (bool success, bytes memory result) {
        bytes memory input = "Hello, precompile!";
        (success, result) = address(0x04).staticcall(input);

        require(success, "Identity precompile call failed");
        require(keccak256(result) == keccak256(input), "Identity precompile returned wrong data");
    }
}
