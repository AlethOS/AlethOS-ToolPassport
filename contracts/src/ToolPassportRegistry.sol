// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract ToolPassportRegistry {
    struct PassportRecord {
        string toolType;
        bytes32 passportHash;
        bytes32 auditLogHash;
        address auditor;
        uint256 timestamp;
    }

    mapping(string toolId => PassportRecord[]) private records;

    event PassportRecorded(
        string indexed toolId,
        string toolType,
        bytes32 passportHash,
        bytes32 auditLogHash,
        address indexed auditor,
        uint256 timestamp
    );

    function recordPassport(
        string calldata toolId,
        string calldata toolType,
        bytes32 passportHash,
        bytes32 auditLogHash
    ) external {
        PassportRecord memory record = PassportRecord({
            toolType: toolType,
            passportHash: passportHash,
            auditLogHash: auditLogHash,
            auditor: msg.sender,
            timestamp: block.timestamp
        });

        records[toolId].push(record);

        emit PassportRecorded(
            toolId, toolType, passportHash, auditLogHash, msg.sender, block.timestamp
        );
    }

    function recordCount(string calldata toolId) external view returns (uint256) {
        return records[toolId].length;
    }

    function recordAt(string calldata toolId, uint256 index)
        external
        view
        returns (PassportRecord memory)
    {
        return records[toolId][index];
    }
}
