// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ToolPassportRegistry} from "../src/ToolPassportRegistry.sol";

contract ToolPassportRegistryTest {
    ToolPassportRegistry private registry;

    function setUp() public {
        registry = new ToolPassportRegistry();
    }

    function testRecordsPassportCommitment() public {
        bytes32 runId = sha256("00000000-0000-0000-0000-000000000001");
        bytes32 passportHash = sha256("passport");
        bytes32 auditLogHash = sha256("audit-log");
        bytes32 evidenceManifestHash = sha256("evidence-manifest");

        registry.recordPassport(
            runId,
            "github:langchain-ai/langgraph",
            "agent_framework",
            passportHash,
            auditLogHash,
            evidenceManifestHash
        );

        require(
            registry.recordCount("github:langchain-ai/langgraph", runId) == 1,
            "record count mismatch"
        );

        ToolPassportRegistry.PassportRecord memory record =
            registry.recordAt("github:langchain-ai/langgraph", runId, 0);
        require(record.passportHash == passportHash, "passport hash mismatch");
        require(record.auditLogHash == auditLogHash, "audit log hash mismatch");
        require(
            record.evidenceManifestHash == evidenceManifestHash, "evidence manifest hash mismatch"
        );
        require(record.auditor == address(this), "auditor mismatch");
    }

    function testSeparatesRunsUnderSameTool() public {
        registry.recordPassport(
            bytes32(uint256(1)),
            "github:langchain-ai/langgraph",
            "agent_framework",
            bytes32(uint256(2)),
            bytes32(uint256(3)),
            bytes32(uint256(4))
        );
        registry.recordPassport(
            bytes32(uint256(5)),
            "github:langchain-ai/langgraph",
            "agent_framework",
            bytes32(uint256(6)),
            bytes32(uint256(7)),
            bytes32(uint256(8))
        );

        require(
            registry.recordCount("github:langchain-ai/langgraph", bytes32(uint256(1))) == 1,
            "first run count mismatch"
        );
        require(
            registry.recordCount("github:langchain-ai/langgraph", bytes32(uint256(5))) == 1,
            "second run count mismatch"
        );

        ToolPassportRegistry.PassportRecord memory first =
            registry.recordAt("github:langchain-ai/langgraph", bytes32(uint256(1)), 0);
        ToolPassportRegistry.PassportRecord memory second =
            registry.recordAt("github:langchain-ai/langgraph", bytes32(uint256(5)), 0);
        require(first.passportHash == bytes32(uint256(2)), "first passport mismatch");
        require(second.passportHash == bytes32(uint256(6)), "second passport mismatch");
    }
}
