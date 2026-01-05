// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract EventTester {
    // Test 1: All log types (LOG0 through LOG4)
    event Log0() anonymous;
    event Log1(uint256 topic1);
    event Log2(uint256 topic1, uint256 topic2);
    event Log3(uint256 topic1, uint256 topic2, uint256 topic3);
    event Log4(uint256 topic1, uint256 topic2, uint256 topic3, uint256 topic4);

    function emitAllLogTypes() public {
        emit Log0();
        emit Log1(1);
        emit Log2(1, 2);
        emit Log3(1, 2, 3);
        emit Log4(1, 2, 3, 4);
    }

    // Test 2: Many logs in a single transaction
    event Counter(uint256 count);

    function emitManyLogs(uint256 count) public {
        for (uint256 i = 0; i < count; i++) {
            emit Counter(i);
        }
    }

    // Test 3: Log with large data payload
    event LargeData(bytes data);

    function emitLargeLog(uint256 dataSize) public {
        bytes memory largeData = new bytes(dataSize);

        // Fill with some pattern data
        for (uint256 i = 0; i < dataSize; i++) {
            largeData[i] = bytes1(uint8(i % 256));
        }

        emit LargeData(largeData);
    }
}
