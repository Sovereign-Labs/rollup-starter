// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";

contract RpcHelper is Script {
    function decodeRpcQuantity(bytes memory raw) internal pure returns (uint256) {
        require(raw.length > 0, "rpc quantity must not be empty");

        if (isHexStringBytes(raw)) {
            return parseHexQuantity(raw);
        }

        (bool ok, bytes memory decodedString) = tryDecodeAbiEncodedHexString(raw);
        if (ok) {
            return parseHexQuantity(decodedString);
        }

        require(raw.length <= 32, "rpc quantity too large");
        uint256 value;
        for (uint256 i = 0; i < raw.length; i++) {
            value = (value << 8) | uint8(raw[i]);
        }
        return value;
    }

    function getBalance(address account) internal returns (uint256) {
        string memory params = string.concat(
            '["', vm.toString(account), '","latest"]'
        );
        bytes memory rpcResult = vm.rpc("eth_getBalance", params);
        return decodeRpcQuantity(rpcResult);
    }

    function isHexStringBytes(bytes memory raw) internal pure returns (bool) {
        return raw.length >= 2 && raw[0] == 0x30 && raw[1] == 0x78;
    }

    function parseHexQuantity(bytes memory raw) internal pure returns (uint256 value) {
        require(raw.length >= 2, "invalid hex quantity");
        require(raw[0] == 0x30 && raw[1] == 0x78, "hex quantity must start with 0x");
        for (uint256 i = 2; i < raw.length; i++) {
            uint8 c = uint8(raw[i]);
            uint8 nibble;
            if (c >= 48 && c <= 57) {
                nibble = c - 48;
            } else if (c >= 97 && c <= 102) {
                nibble = c - 87;
            } else if (c >= 65 && c <= 70) {
                nibble = c - 55;
            } else {
                revert("invalid hex digit");
            }
            value = (value << 4) | uint256(nibble);
        }
    }

    function tryDecodeAbiEncodedHexString(bytes memory raw) internal pure returns (bool, bytes memory) {
        if (raw.length < 96 || raw.length % 32 != 0) {
            return (false, bytes(""));
        }

        uint256 offset;
        uint256 len;
        assembly {
            offset := mload(add(raw, 0x20))
            len := mload(add(raw, 0x40))
        }
        if (offset != 0x20 || len < 2) {
            return (false, bytes(""));
        }

        uint256 paddedLen = ((len + 31) / 32) * 32;
        if (raw.length != 64 + paddedLen) {
            return (false, bytes(""));
        }

        bytes memory decoded = new bytes(len);
        for (uint256 i = 0; i < len; i++) {
            decoded[i] = raw[64 + i];
        }
        if (!isHexStringBytes(decoded)) {
            return (false, bytes(""));
        }

        return (true, decoded);
    }
}
