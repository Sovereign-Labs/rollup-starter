// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract ValueTransferTester {
    uint256 public totalReceived;

    receive() external payable {
        totalReceived += msg.value;
    }

    fallback() external payable {
        totalReceived += msg.value;
    }

    function forwardTo(address payable dest) public payable {
        (bool success,) = dest.call{value: msg.value}("");
        require(success, "Forward failed");
    }

    function getBalance() public view returns (uint256) {
        return address(this).balance;
    }
}

contract ValueRejecter {
    // No receive or fallback — rejects ETH
}

contract ValueReceiver {
    receive() external payable {}

    function getBalance() public view returns (uint256) {
        return address(this).balance;
    }
}
