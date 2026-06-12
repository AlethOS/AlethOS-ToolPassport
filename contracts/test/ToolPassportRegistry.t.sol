// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ToolPassportRegistry} from "../src/ToolPassportRegistry.sol";

contract ToolPassportRegistryTest {
    ToolPassportRegistry private registry;

    function setUp() public {
        registry = new ToolPassportRegistry();
    }

    function testRecordsPassportCommitment() public {
        bytes32 passportHash = keccak256("passport");
        bytes32 auditLogHash = keccak256("audit-log");

        registry.recordPassport("langgraph", "agent_framework", passportHash, auditLogHash);

        require(registry.recordCount("langgraph") == 1, "record count mismatch");

        ToolPassportRegistry.PassportRecord memory record = registry.recordAt("langgraph", 0);
        require(record.passportHash == passportHash, "passport hash mismatch");
        require(record.auditLogHash == auditLogHash, "audit log hash mismatch");
        require(record.auditor == address(this), "auditor mismatch");
    }

    function testAllowsMultipleRecords() public {
        registry.recordPassport(
            "langgraph", "agent_framework", bytes32(uint256(1)), bytes32(uint256(2))
        );
        registry.recordPassport(
            "langgraph", "agent_framework", bytes32(uint256(3)), bytes32(uint256(4))
        );

        require(registry.recordCount("langgraph") == 2, "record count mismatch");
    }
}
