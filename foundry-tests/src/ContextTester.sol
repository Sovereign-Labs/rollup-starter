// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract ContextTester {
    function getBlockContext() public view returns (
        uint256 blockNumber,
        uint256 blockTimestamp,
        uint256 blockGaslimit,
        uint256 blockChainid,
        uint256 blockBasefee,
        uint256 blockPrevrandao
    ) {
        blockNumber = block.number;
        blockTimestamp = block.timestamp;
        blockGaslimit = block.gaslimit;
        blockChainid = block.chainid;
        blockBasefee = block.basefee;
        blockPrevrandao = block.prevrandao;
    }

    function getTxContext() public view returns (
        address msgSender,
        address txOrigin,
        uint256 txGasprice
    ) {
        msgSender = msg.sender;
        txOrigin = tx.origin;
        txGasprice = tx.gasprice;
    }
}
