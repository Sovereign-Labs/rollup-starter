// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
abstract contract RpcAssertions is Script {
    address internal constant DEFAULT_FROM = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

    function rpcResult(string memory method, string memory params) internal returns (bytes memory) {
        return vm.rpc(method, params);
    }

    function rpcJson(string memory method, string memory params) internal returns (string memory) {
        return decodeRpcJson(rpcResult(method, params));
    }

    function rpcJsonByFfi(string memory method, string memory params) internal returns (string memory) {
        return rpcJsonByFfi("sovereign", method, params);
    }

    function rpcJsonByFfi(string memory rpcAlias, string memory method, string memory params)
        internal
        returns (string memory)
    {
        string memory payload =
            string.concat('{"jsonrpc":"2.0","id":1,"method":"', method, '","params":', params, "}");

        string[] memory command = new string[](6);
        command[0] = "sh";
        command[1] = "-c";
        command[2] = "curl -sS -H 'content-type: application/json' --data \"$1\" \"$2\" | jq -c '.result'";
        command[3] = "_";
        command[4] = payload;
        command[5] = vm.rpcUrl(rpcAlias);

        bytes memory output = vm.ffi(command);
        require(output.length > 0, "ffi rpc result must not be empty");
        return trimTrailingNewline(string(output));
    }

    function decodeRpcJson(bytes memory raw) internal pure returns (string memory) {
        require(raw.length > 0, "rpc json result must not be empty");
        if (isJsonTextBytes(raw)) {
            return string(raw);
        }

        (bool ok, string memory decoded) = tryDecodeAbiEncodedString(raw);
        require(ok, "rpc result is not json text");
        return decoded;
    }

    function jsonString(string memory json, string memory key, string memory label) internal returns (string memory) {
        try vm.parseJsonString(json, key) returns (string memory value) {
            return value;
        } catch {
            revert(string.concat("missing/invalid json string: ", label));
        }
    }

    function jsonBool(string memory json, string memory key, string memory label) internal returns (bool) {
        try vm.parseJsonBool(json, key) returns (bool value) {
            return value;
        } catch {
            revert(string.concat("missing/invalid json bool: ", label));
        }
    }

    function jsonKeys(string memory json, string memory key, string memory label) internal returns (string[] memory) {
        try vm.parseJsonKeys(json, key) returns (string[] memory keys) {
            return keys;
        } catch {
            revert(string.concat("missing/invalid json object/array keys: ", label));
        }
    }

    function jsonPathExists(string memory json, string memory key) internal returns (bool) {
        try vm.parseJson(json, key) returns (bytes memory) {
            return true;
        } catch {
            return false;
        }
    }

    function waitForReceiptJson(string memory txHash, uint256 attempts, uint256 sleepMs)
        internal
        returns (string memory)
    {
        string memory params = string.concat('["', txHash, '"]');
        for (uint256 i = 0; i < attempts; i++) {
            string memory receiptJson = rpcJson("eth_getTransactionReceipt", params);
            if (!isJsonNull(receiptJson)) {
                return receiptJson;
            }
            vm.sleep(sleepMs);
        }
        revert("timed out waiting for transaction receipt");
    }

    function decodeRpcQuantity(bytes memory raw) internal pure returns (uint256) {
        require(raw.length > 0, "rpc quantity must not be empty");

        if (isHexStringBytes(raw)) {
            return parseHexQuantityBytes(raw);
        }

        (bool ok, bytes memory decodedString) = tryDecodeAbiEncodedHexString(raw);
        if (ok) {
            return parseHexQuantityBytes(decodedString);
        }

        require(raw.length <= 32, "rpc quantity too large");
        uint256 value;
        for (uint256 i = 0; i < raw.length; i++) {
            value = (value << 8) | uint8(raw[i]);
        }
        return value;
    }

    function parseHexQuantityString(string memory value) internal pure returns (uint256) {
        return parseHexQuantityBytes(bytes(value));
    }

    function parseHexQuantityBytes(bytes memory raw) internal pure returns (uint256 value) {
        require(raw.length >= 3, "invalid hex quantity");
        require(raw[0] == 0x30 && raw[1] == 0x78, "hex quantity must start with 0x");
        for (uint256 i = 2; i < raw.length; i++) {
            value = (value << 4) | uint256(hexNibble(raw[i]));
        }
    }

    function normalizeRpcHexData(bytes memory raw) internal pure returns (bytes memory) {
        if (isHexStringBytes(raw)) {
            return parseHexDataBytes(raw);
        }

        (bool ok, bytes memory decodedString) = tryDecodeAbiEncodedHexString(raw);
        if (ok) {
            return parseHexDataBytes(decodedString);
        }

        return raw;
    }

    function parseHexDataString(string memory value) internal pure returns (bytes memory) {
        return parseHexDataBytes(bytes(value));
    }

    function parseHexDataBytes(bytes memory raw) internal pure returns (bytes memory out) {
        require(raw.length >= 2, "invalid hex data");
        require(raw[0] == 0x30 && raw[1] == 0x78, "hex data must start with 0x");
        require((raw.length - 2) % 2 == 0, "hex data must have even digit count");

        out = new bytes((raw.length - 2) / 2);
        for (uint256 i = 2; i < raw.length; i += 2) {
            uint8 hi = hexNibble(raw[i]);
            uint8 lo = hexNibble(raw[i + 1]);
            out[(i - 2) / 2] = bytes1((hi << 4) | lo);
        }
    }

    function selectorOf(bytes memory data) internal pure returns (bytes4 sel) {
        require(data.length >= 4, "data too short for selector");
        assembly {
            sel := mload(add(data, 32))
        }
    }

    function toHexQuantity(uint256 value) internal pure returns (string memory) {
        if (value == 0) {
            return "0x0";
        }

        bytes memory alphabet = "0123456789abcdef";
        bytes memory buffer = new bytes(64);
        uint256 len = 0;
        while (value > 0) {
            buffer[63 - len] = alphabet[value & 0xf];
            value >>= 4;
            len++;
        }

        bytes memory out = new bytes(len + 2);
        out[0] = 0x30;
        out[1] = 0x78;
        for (uint256 i = 0; i < len; i++) {
            out[2 + i] = buffer[64 - len + i];
        }
        return string(out);
    }

    function ethBalance(address account) internal returns (uint256) {
        string memory params = string.concat('["', vm.toString(account), '","latest"]');
        return decodeRpcQuantity(rpcResult("eth_getBalance", params));
    }

    function currentBlockNumber() internal returns (uint256) {
        return decodeRpcQuantity(rpcResult("eth_blockNumber", "[]"));
    }

    function assertHexQuantityString(string memory value, string memory label) internal pure {
        bytes memory raw = bytes(value);
        require(raw.length >= 3, string.concat(label, " must start with 0x and have digits"));
        require(raw[0] == 0x30 && raw[1] == 0x78, string.concat(label, " must start with 0x"));
        require(!(raw.length > 3 && raw[2] == 0x30), string.concat(label, " has non-canonical leading zero"));
        for (uint256 i = 2; i < raw.length; i++) {
            require(isHexChar(raw[i]), string.concat(label, " contains non-hex characters"));
        }
    }

    function assertHexDataString(string memory value, string memory label) internal pure {
        bytes memory raw = bytes(value);
        require(raw.length >= 2, string.concat(label, " must start with 0x"));
        require(raw[0] == 0x30 && raw[1] == 0x78, string.concat(label, " must start with 0x"));
        require((raw.length - 2) % 2 == 0, string.concat(label, " data payload must have even hex length"));
        for (uint256 i = 2; i < raw.length; i++) {
            require(isHexChar(raw[i]), string.concat(label, " contains non-hex characters"));
        }
    }

    function assertHexHashString(string memory value, string memory label) internal pure {
        bytes memory raw = bytes(value);
        require(raw.length == 66, string.concat(label, " must be 32-byte hash"));
        require(raw[0] == 0x30 && raw[1] == 0x78, string.concat(label, " must start with 0x"));
        for (uint256 i = 2; i < raw.length; i++) {
            require(isHexChar(raw[i]), string.concat(label, " contains non-hex characters"));
        }
    }

    function assertHexAddressString(string memory value, string memory label) internal pure {
        bytes memory raw = bytes(value);
        require(raw.length == 42, string.concat(label, " must be 20-byte address"));
        require(raw[0] == 0x30 && raw[1] == 0x78, string.concat(label, " must start with 0x"));
        for (uint256 i = 2; i < raw.length; i++) {
            require(isHexChar(raw[i]), string.concat(label, " contains non-hex characters"));
        }
    }

    function isJsonNull(string memory json) internal pure returns (bool) {
        return keccak256(bytes(json)) == keccak256(bytes("null"));
    }

    function isHexChar(bytes1 c) internal pure returns (bool) {
        return (c >= 0x30 && c <= 0x39) || (c >= 0x61 && c <= 0x66) || (c >= 0x41 && c <= 0x46);
    }

    function hexNibble(bytes1 c) internal pure returns (uint8) {
        uint8 b = uint8(c);
        if (b >= 48 && b <= 57) return b - 48;
        if (b >= 97 && b <= 102) return b - 87;
        if (b >= 65 && b <= 70) return b - 55;
        revert("invalid hex digit");
    }

    function isHexStringBytes(bytes memory raw) internal pure returns (bool) {
        return raw.length >= 2 && raw[0] == 0x30 && raw[1] == 0x78;
    }

    function isJsonTextBytes(bytes memory raw) internal pure returns (bool) {
        if (raw.length == 0) return false;
        bytes1 first = raw[0];
        // JSON object / array / string / null
        return first == 0x7b || first == 0x5b || first == 0x22 || first == 0x6e;
    }

    function tryDecodeAbiEncodedString(bytes memory raw) internal pure returns (bool, string memory) {
        if (raw.length < 96 || raw.length % 32 != 0) {
            return (false, "");
        }

        uint256 offset;
        uint256 len;
        assembly {
            offset := mload(add(raw, 0x20))
            len := mload(add(raw, 0x40))
        }
        if (offset != 0x20) {
            return (false, "");
        }

        uint256 paddedLen = ((len + 31) / 32) * 32;
        if (raw.length != 64 + paddedLen) {
            return (false, "");
        }

        bytes memory decoded = new bytes(len);
        for (uint256 i = 0; i < len; i++) {
            decoded[i] = raw[64 + i];
        }
        return (true, string(decoded));
    }

    function tryDecodeAbiEncodedHexString(bytes memory raw) internal pure returns (bool, bytes memory) {
        (bool ok, string memory decoded) = tryDecodeAbiEncodedString(raw);
        if (!ok) {
            return (false, bytes(""));
        }

        bytes memory asBytes = bytes(decoded);
        if (!isHexStringBytes(asBytes)) {
            return (false, bytes(""));
        }
        return (true, asBytes);
    }

    function trimTrailingNewline(string memory value) internal pure returns (string memory) {
        bytes memory raw = bytes(value);
        uint256 end = raw.length;
        while (end > 0 && (raw[end - 1] == 0x0a || raw[end - 1] == 0x0d)) {
            end--;
        }
        if (end == raw.length) {
            return value;
        }

        bytes memory trimmed = new bytes(end);
        for (uint256 i = 0; i < end; i++) {
            trimmed[i] = raw[i];
        }
        return string(trimmed);
    }
}
