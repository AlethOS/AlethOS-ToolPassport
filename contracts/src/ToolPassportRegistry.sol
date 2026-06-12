// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract ToolPassportRegistry {
    struct PassportRecord {
        string toolType;
        bytes32 passportHash;
        bytes32 auditLogHash;
        bytes32 evidenceManifestHash;
        address auditor;
        uint256 timestamp;
    }

    mapping(string toolId => mapping(bytes32 runId => PassportRecord[])) private records;

    event PassportRecorded(
        string indexed toolId,
        bytes32 indexed runId,
        string toolType,
        bytes32 passportHash,
        bytes32 auditLogHash,
        bytes32 evidenceManifestHash,
        address indexed auditor,
        uint256 timestamp
    );

    function recordPassport(
        bytes32 runId,
        string calldata toolId,
        string calldata toolType,
        bytes32 passportHash,
        bytes32 auditLogHash,
        bytes32 evidenceManifestHash
    ) external {
        PassportRecord memory record = PassportRecord({
            toolType: toolType,
            passportHash: passportHash,
            auditLogHash: auditLogHash,
            evidenceManifestHash: evidenceManifestHash,
            auditor: msg.sender,
            timestamp: block.timestamp
        });

        records[toolId][runId].push(record);

        emit PassportRecorded(
            toolId,
            runId,
            toolType,
            passportHash,
            auditLogHash,
            evidenceManifestHash,
            msg.sender,
            block.timestamp
        );
    }

    function recordCount(string calldata toolId, bytes32 runId) external view returns (uint256) {
        return records[toolId][runId].length;
    }

    function recordAt(string calldata toolId, bytes32 runId, uint256 index)
        external
        view
        returns (PassportRecord memory)
    {
        return records[toolId][runId][index];
    }
}
