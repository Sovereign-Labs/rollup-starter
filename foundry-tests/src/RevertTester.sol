// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract RevertTester {
    error CustomError(uint256 code, string message);
    error SimpleError();

    uint256 public value;

    function revertWithRequire() public pure {
        require(false, "require failed");
    }

    function revertWithCustomError(uint256 code) public pure {
        revert CustomError(code, "custom error");
    }

    function revertEmpty() public pure {
        revert();
    }

    function revertSimpleError() public pure {
        revert SimpleError();
    }

    function conditionalRevert(bool shouldRevert) public {
        value = 42;
        if (shouldRevert) {
            revert("conditional revert");
        }
    }

    function stateChangeAndRevert() public {
        value = 999;
        revert("state change reverted");
    }

    function tryCallRevert() public returns (bool success, bytes memory returnData) {
        (success, returnData) = address(this).call(
            abi.encodeWithSignature("revertWithRequire()")
        );
    }

    function overflowUnchecked() public pure returns (uint256) {
        unchecked {
            uint256 x = type(uint256).max;
            return x + 1;
        }
    }

    function assertFailure() public pure {
        assert(false);
    }

    function divisionByZero() public pure returns (uint256) {
        uint256 a = 1;
        uint256 b = 0;
        return a / b; // Panic(0x12)
    }
}
