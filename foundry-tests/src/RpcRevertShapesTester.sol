// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract RpcRevertShapesTester {
    error CustomFailure(uint256 code, string message);
    error SimpleFailure();

    uint256 public value;

    function setValue(uint256 newValue) external {
        value = newValue;
    }

    function revertWithRequire() external pure {
        require(false, "rpc require failed");
    }

    function revertWithCustom(uint256 code) external pure {
        revert CustomFailure(code, "custom failure");
    }

    function revertWithSimple() external pure {
        revert SimpleFailure();
    }

    function revertWithPanic() external pure {
        assert(false);
    }

    function revertEmpty() external pure {
        revert();
    }
}
