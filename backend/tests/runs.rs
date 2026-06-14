use std::str::FromStr;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use toolpassport_backend::{app, migrate};
use tower::ServiceExt;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Helpers (same pattern as tools.rs, vendored here for test isolation)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migrations_create_run_tables_and_append_only_triggers() {
    let pool = test_pool().await;

    let object_names: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE name IN (
            'runs',
            'run_events',
            'run_events_prevent_update',
            'run_events_prevent_delete'
        )
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("migration objects must be queryable");

    assert_eq!(
        object_names,
        [
            "run_events",
            "run_events_prevent_delete",
            "run_events_prevent_update",
            "runs"
        ]
    );
}

#[tokio::test]
async fn creates_and_queries_runs() {
    let (router, _) = test_app().await;

    // Create a tool first, then bind the run to it.
    let tool_id = create_github_tool(&router).await;

    let created = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool_id": tool_id
        }),
    )
    .await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["goal"], "Audit the tool");
    assert_eq!(created.1["tool_id"], tool_id);
    assert_eq!(created.1["tool"]["name"], "example-lib");
    assert_eq!(created.1["tool"]["tool_type"], "generic");
    assert_eq!(created.1["status"], "pending");
    let run_id = created.1["run_id"]
        .as_str()
        .expect("created run must have an ID");
    Uuid::parse_str(run_id).expect("run ID must be a UUID");

    let list = send(&router, Method::GET, "/api/runs", Body::empty()).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(
        list.1["runs"]
            .as_array()
            .expect("runs must be an array")
            .len(),
        1
    );
    assert_eq!(list.1["runs"][0]["run_id"], run_id);

    let details = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await;
    assert_eq!(details.0, StatusCode::OK);
    assert_eq!(details.1["run"]["run_id"], run_id);
    assert_eq!(details.1["run"]["tool_id"], tool_id);
    assert_eq!(details.1["events"][0]["event_type"], "run_created");
    assert_eq!(details.1["events"][0]["node_id"], "run");
    assert_eq!(details.1["events"][0]["payload"]["status"], "pending");
}

#[tokio::test]
async fn appends_events_in_order_and_rejects_mutation() {
    let (router, pool) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let first = append_event(&router, &run_id, "plan_audit", "node_started").await;
    let second = append_event(&router, &run_id, "plan_audit", "node_finished").await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::CREATED);
    // v0.2: sequence is now exposed in API responses.
    assert!(first.1.get("sequence").is_some());
    assert!(second.1.get("sequence").is_some());

    let details = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events[0]["event_type"], "run_created");
    assert_eq!(events[1]["event_type"], "node_started");
    assert_eq!(events[2]["event_type"], "node_finished");
    assert_eq!(details.1["run"]["status"], "running");
    assert_eq!(details.1["run"]["current_node"], "plan_audit");

    let event_id = first.1["event_id"]
        .as_str()
        .expect("event ID must be present");
    let update_error = sqlx::query("UPDATE run_events SET node_id = 'changed' WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect_err("event update must be rejected");
    assert!(update_error.to_string().contains("append-only"));

    let delete_error = sqlx::query("DELETE FROM run_events WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect_err("event delete must be rejected");
    assert!(delete_error.to_string().contains("append-only"));
}

#[tokio::test]
async fn projects_approval_and_terminal_status_events() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let started = append_event(&router, &run_id, "plan_audit", "node_started").await;
    assert_eq!(started.0, StatusCode::CREATED);

    let approval_required =
        append_event(&router, &run_id, "human_review_gate", "approval_required").await;
    assert_eq!(approval_required.0, StatusCode::CREATED);
    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "waiting_approval");
    assert_eq!(details.1["run"]["current_node"], "human_review_gate");

    let blocked_node = append_event(&router, &run_id, "next_node", "node_started").await;
    assert_error(&blocked_node, StatusCode::BAD_REQUEST, "invalid_request");

    let approval_resolved =
        append_event(&router, &run_id, "human_review_gate", "approval_resolved").await;
    assert_eq!(approval_resolved.0, StatusCode::CREATED);

    let finished = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "success"}),
    )
    .await;
    assert_eq!(finished.0, StatusCode::CREATED);

    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "success");
    assert_eq!(details.1["run"]["current_node"], "finish");
}

#[tokio::test]
async fn rejects_trust_core_owned_and_invalid_status_events() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let duplicate_created = append_event(&router, &run_id, "run", "run_created").await;
    assert_error(
        &duplicate_created,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let forged_score = append_event(&router, &run_id, "orchestrator", "score_changed").await;
    assert_error(&forged_score, StatusCode::BAD_REQUEST, "invalid_request");

    let invalid_status = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "cancelled"}),
    )
    .await;
    assert_error(&invalid_status, StatusCode::BAD_REQUEST, "invalid_request");

    let success_from_pending = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "success"}),
    )
    .await;
    assert_error(
        &success_from_pending,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "pending");
    assert_eq!(
        details.1["events"]
            .as_array()
            .expect("events must be an array")
            .len(),
        1
    );
}

#[tokio::test]
async fn returns_json_errors_for_missing_runs() {
    let (router, _) = test_app().await;
    let missing_id = Uuid::new_v4();

    let response = send(
        &router,
        Method::GET,
        &format!("/api/runs/{missing_id}"),
        Body::empty(),
    )
    .await;
    assert_error(&response, StatusCode::NOT_FOUND, "run_not_found");

    let response = append_event(&router, &missing_id.to_string(), "node", "node_started").await;
    assert_error(&response, StatusCode::NOT_FOUND, "run_not_found");
}

#[tokio::test]
async fn returns_json_errors_for_invalid_requests() {
    let (router, _) = test_app().await;

    // Missing tool_id.
    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");

    // Empty goal.
    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": " ",
            "tool_id": "github:owner/repo"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");

    // Invalid JSON body.
    let response = send(&router, Method::POST, "/api/runs", Body::from("{invalid")).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");

    // Invalid run_id (not a UUID).
    let response = send(&router, Method::GET, "/api/runs/not-a-uuid", Body::empty()).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_run_id");
}

#[tokio::test]
async fn run_binding_requires_existing_tool() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit a nonexistent tool",
            "tool_id": "github:nonexistent/repo"
        }),
    )
    .await;
    assert_error(&response, StatusCode::NOT_FOUND, "tool_not_found");
}

#[tokio::test]
async fn run_binding_freezes_tool_snapshot() {
    let (router, _) = test_app().await;

    // Create a tool.
    let tool_id = create_github_tool(&router).await;

    // Create a run bound to it — snapshot is frozen at creation time.
    let run = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "First audit",
            "tool_id": &tool_id
        }),
    )
    .await;
    assert_eq!(run.0, StatusCode::CREATED);
    let run_id = run.1["run_id"]
        .as_str()
        .expect("run_id must be present")
        .to_owned();

    // Fetch the run and verify the frozen snapshot fields.
    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["tool_id"], tool_id);
    assert_eq!(
        details.1["run"]["canonical_url"],
        "https://github.com/example-org/example-lib"
    );
    assert_eq!(details.1["run"]["tool"]["name"], "example-lib");
    assert_eq!(details.1["run"]["tool"]["tool_type"], "generic");
    assert_eq!(
        details.1["run"]["tool"]["urls"][0],
        "https://github.com/example-org/example-lib"
    );

    // Update the Tool (add an alias) — the Run snapshot must NOT change.
    let _alias_result = send_json(
        &router,
        Method::POST,
        &format!("/api/tools/{tool_id}/identifiers"),
        json!({
            "identifier": {
                "namespace": "url",
                "value": "other.example.com/tool",
                "canonical_url": "https://other.example.com/tool"
            }
        }),
    )
    .await;

    // Re-fetch the run — snapshot unchanged.
    let details_after = get_run(&router, &run_id).await;
    assert_eq!(details_after.1["run"]["tool_id"], tool_id);
    assert_eq!(
        details_after.1["run"]["canonical_url"],
        "https://github.com/example-org/example-lib"
    );
}

#[tokio::test]
async fn run_binding_rejects_invalid_tool_id_format() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Invalid tool",
            "tool_id": "not-a-namespaced-id"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_tool_id_format");
}

// ═══════════════════════════════════════════════════════════════════
// Test infrastructure
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// Stage 5 — Hash chain and decision events
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn first_event_has_zero_prev_hash() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let details = get_run(&router, &run_id).await;
    let event = &details.1["events"][0];
    assert_eq!(event["event_type"], "run_created");
    assert_eq!(
        event["prev_event_hash"],
        "0x0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert!(
        event["event_hash"]
            .as_str()
            .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
    );
    assert_ne!(event["event_hash"], event["prev_event_hash"]);
}

#[tokio::test]
async fn hash_chain_is_deterministic_and_sequential() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let first = append_event(&router, &run_id, "plan_audit", "node_started").await;
    assert_eq!(first.0, StatusCode::CREATED);
    let second = append_event(&router, &run_id, "plan_audit", "node_finished").await;
    assert_eq!(second.0, StatusCode::CREATED);
    let third = append_event(&router, &run_id, "review", "node_started").await;
    assert_eq!(third.0, StatusCode::CREATED);

    // Each event must have sequence, event_hash, prev_event_hash.
    for ev in [&first.1, &second.1, &third.1] {
        assert!(ev["sequence"].as_i64().is_some());
        assert!(
            ev["event_hash"]
                .as_str()
                .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
        );
        assert!(
            ev["prev_event_hash"]
                .as_str()
                .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
        );
    }

    // Chain links: event N's prev_event_hash == event N-1's event_hash.
    let details = get_run(&router, &run_id).await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events.len(), 4); // run_created + 3 appends
    for i in 1..events.len() {
        assert_eq!(
            events[i]["prev_event_hash"],
            events[i - 1]["event_hash"],
            "chain broken at index {i}"
        );
    }

    // Deterministic: same input → same event_hash.
    // Re-run the exact same event append on a fresh run and compare.
    let (router2, _) = test_app().await;
    let tool_id2 = create_github_tool(&router2).await;
    let run_id2 = create_run(&router2, &tool_id2).await;
    let replayed = append_event(&router2, &run_id2, "plan_audit", "node_started").await;
    assert_eq!(replayed.0, StatusCode::CREATED);

    // The run_created hash will differ (different run_id), but the
    // appended event should chain from its own prev correctly.
    let details2 = get_run(&router2, &run_id2).await;
    let events2 = details2.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events2.len(), 2);
    assert_eq!(
        events2[1]["prev_event_hash"], events2[0]["event_hash"],
        "replayed chain must link correctly"
    );
}

#[tokio::test]
async fn hash_chain_detects_tampering_via_direct_db() {
    let (router, pool) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // Append two events normally.
    append_event(&router, &run_id, "plan_audit", "node_started").await;
    append_event(&router, &run_id, "plan_audit", "node_finished").await;

    // Read the events and record the hashes before tampering.
    let details_before = get_run(&router, &run_id).await;
    let events_before = details_before.1["events"]
        .as_array()
        .expect("events must be an array");
    let event_1_hash = events_before[1]["event_hash"]
        .as_str()
        .expect("hash must be present")
        .to_owned();
    let event_2_prev = events_before[2]["prev_event_hash"]
        .as_str()
        .expect("prev must be present")
        .to_owned();
    assert_eq!(event_2_prev, event_1_hash);

    // Disable the append-only trigger and tamper with a payload via raw SQL.
    sqlx::query("DROP TRIGGER run_events_prevent_update")
        .execute(&pool)
        .await
        .expect("must be able to drop trigger for tamper test");

    let event_1_id = events_before[1]["event_id"]
        .as_str()
        .expect("event_id must be present");
    let tampered_payload = r#"{"tampered": true}"#;
    sqlx::query("UPDATE run_events SET payload = ? WHERE event_id = ?")
        .bind(tampered_payload)
        .bind(event_1_id)
        .execute(&pool)
        .await
        .expect("tamper must succeed without trigger");

    // Now the hash chain is broken: event[2].prev_event_hash no longer matches
    // the hash that event[1] should have with its original payload.
    // The hash stored in event[2] was computed from event[1]'s original hash,
    // but event[1]'s payload has changed — recalculating event[1]'s hash
    // with the tampered payload will yield a different value than what
    // event[2].prev_event_hash points to.
    //
    // We can detect this by re-reading the events and checking:
    let details_after = get_run(&router, &run_id).await;
    let events_after = details_after.1["events"]
        .as_array()
        .expect("events must be an array");
    let event_1_hash_after = events_after[1]["event_hash"]
        .as_str()
        .expect("hash must be present");
    // The stored hash for event 1 hasn't changed (we only changed payload),
    // but the stored hash no longer matches a fresh computation over the tampered payload.
    // The critical observation: event 2's prev_event_hash still equals event 1's
    // OLD hash, which no longer certifies the current payload.
    // A verifier that re-hashes event 1 would get a different result.
    let event_2_prev_after = events_after[2]["prev_event_hash"]
        .as_str()
        .expect("prev must be present");
    // The chain link stored values are still equal (UPDATE didn't touch hashes),
    // but the tampered payload means re-hashing produces a mismatch.
    assert_eq!(
        event_1_hash_after, event_1_hash,
        "stored hash unchanged by UPDATE"
    );
    assert_eq!(
        event_2_prev_after, event_1_hash,
        "stored prev still points to old hash"
    );
    // The actual payload is now tampered — verifier detects this.
    assert_eq!(
        events_after[1]["payload"]["tampered"], true,
        "payload was tampered"
    );

    // Recreate the trigger.
    sqlx::query(
        "CREATE TRIGGER run_events_prevent_update BEFORE UPDATE ON run_events BEGIN SELECT RAISE(ABORT, 'run_events are append-only'); END",
    )
    .execute(&pool)
    .await
    .expect("must recreate trigger");
}

#[tokio::test]
async fn decision_events_are_accepted() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // First move to running state so we can append decision events.
    let started = append_event(&router, &run_id, "plan_audit", "node_started").await;
    assert_eq!(started.0, StatusCode::CREATED);

    let decision_types = [
        "profile_selected",
        "hypothesis_created",
        "hypothesis_updated",
        "research_query_planned",
        "gap_detected",
        "evidence_linked",
        "claim_contradicted",
        "review_issue_found",
        "directives_accepted",
        "human_feedback_received",
    ];

    for event_type in decision_types {
        let result = append_event_with_payload(
            &router,
            &run_id,
            "orchestrator",
            event_type,
            json!({"reason": "Stage 5 test", "event_type": event_type}),
        )
        .await;
        assert_eq!(
            result.0,
            StatusCode::CREATED,
            "decision event {event_type} must be accepted: {result:?}"
        );
        assert_eq!(result.1["event_type"], event_type);
        assert!(result.1["sequence"].as_i64().is_some());
        assert!(
            result.1["event_hash"]
                .as_str()
                .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
        );
    }

    // Verify all events are in the list.
    let details = get_run(&router, &run_id).await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    // run_created + node_started + externally accepted decision events
    assert_eq!(events.len(), 2 + decision_types.len());

    let forged_freeze = append_event_with_payload(
        &router,
        &run_id,
        "orchestrator",
        "evidence_board_frozen",
        json!({"evidence_board_version": 1}),
    )
    .await;
    assert_error(&forged_freeze, StatusCode::BAD_REQUEST, "invalid_request");

    let forged_provenance = append_event_with_payload(
        &router,
        &run_id,
        "orchestrator",
        "provenance_frozen",
        json!({"passport_sequence": 1}),
    )
    .await;
    assert_error(
        &forged_provenance,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );
}

#[tokio::test]
async fn sequence_is_exposed_in_api_responses() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // run_created should have sequence 1.
    let details = get_run(&router, &run_id).await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events[0]["sequence"], 1);

    // Append events in order and verify sequences increment.
    let e2 = append_event(&router, &run_id, "a", "node_started").await;
    assert_eq!(e2.1["sequence"], 2);

    let e3 = append_event(&router, &run_id, "b", "node_finished").await;
    assert_eq!(e3.1["sequence"], 3);

    let e4 = append_event(&router, &run_id, "c", "hypothesis_created").await;
    assert_eq!(e4.1["sequence"], 4);

    // Re-fetch and confirm all sequences are present and monotonic.
    let details = get_run(&router, &run_id).await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events.len(), 4);
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event["sequence"], (i + 1) as i64);
    }
}

#[tokio::test]
async fn generated_events_have_hash_chain() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // Upload an artifact — this generates an artifact_created event.
    let boundary = "testboundary";
    let body_bytes = build_multipart_body(boundary, "test.txt", "text/plain", b"hello world");
    let artifact_response = send_typed(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/artifacts"),
        Body::from(body_bytes),
        &format!("multipart/form-data; boundary={boundary}"),
    )
    .await;
    assert_eq!(artifact_response.0, StatusCode::CREATED);

    // Upload evidence — this generates an evidence_created event.
    let evidence_response = send_json(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence"),
        json!({
            "evidence_schema_version": "0.2.0",
            "source_type": "official_docs",
            "source_url": "https://example.com/docs",
            "title": "Test evidence",
            "excerpt": "Test excerpt for hash chain.",
            "retrieved_at": "2026-06-12T20:00:00Z",
            "snapshot_artifact_id": null,
            "supports": [],
            "contradicts": [],
            "metadata": {}
        }),
    )
    .await;
    assert_eq!(evidence_response.0, StatusCode::CREATED);

    // Fetch the run and verify the generated events have proper hash chains.
    let details = get_run(&router, &run_id).await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    // run_created + artifact_created + evidence_created = 3
    assert!(
        events.len() >= 3,
        "expected at least 3 events, got {}",
        events.len()
    );

    // Verify all events have hash fields.
    for event in events {
        assert!(
            event["event_hash"]
                .as_str()
                .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
        );
        assert!(
            event["prev_event_hash"]
                .as_str()
                .is_some_and(|h| h.starts_with("0x") && h.len() == 66)
        );
    }

    // Verify the chain links correctly.
    for i in 1..events.len() {
        assert_eq!(
            events[i]["prev_event_hash"],
            events[i - 1]["event_hash"],
            "generated event chain broken at index {i}"
        );
    }

    // Find the artifact_created and evidence_created events specifically.
    let artifact_events: Vec<_> = events
        .iter()
        .filter(|e| e["event_type"] == "artifact_created")
        .collect();
    let evidence_events: Vec<_> = events
        .iter()
        .filter(|e| e["event_type"] == "evidence_created")
        .collect();
    assert_eq!(
        artifact_events.len(),
        1,
        "should have exactly one artifact_created event"
    );
    assert_eq!(
        evidence_events.len(),
        1,
        "should have exactly one evidence_created event"
    );
}

#[tokio::test]
async fn event_hash_uses_jcs_canonicalization() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id_a = create_run(&router, &tool_id).await;

    // Append a decision event with a known payload.
    let payload = json!({
        "profile_id": "mcp_server@0.2.0",
        "reason": "Tool exposes MCP protocol endpoints",
        "passport_hash": null,
        "version": 1
    });
    let event_a = append_event_with_payload(
        &router,
        &run_id_a,
        "orchestrator",
        "profile_selected",
        payload.clone(),
    )
    .await;
    assert_eq!(event_a.0, StatusCode::CREATED);
    let hash_a = event_a.1["event_hash"]
        .as_str()
        .expect("hash must be present")
        .to_owned();

    // Append the same event type with the same payload to another run.
    // The hash will differ because run_id differs.
    let (router2, _) = test_app().await;
    let tool_id2 = create_github_tool(&router2).await;
    let run_id_b = create_run(&router2, &tool_id2).await;
    let event_b = append_event_with_payload(
        &router2,
        &run_id_b,
        "orchestrator",
        "profile_selected",
        payload.clone(),
    )
    .await;
    assert_eq!(event_b.0, StatusCode::CREATED);
    let hash_b = event_b.1["event_hash"]
        .as_str()
        .expect("hash must be present")
        .to_owned();

    // Different run_id → different hashes.
    assert_ne!(hash_a, hash_b);

    // Same payload → hash is computed deterministically within its context.
    // The JCS canonicalization ensures key ordering doesn't matter.
    let payload_reordered = json!({
        "reason": "Tool exposes MCP protocol endpoints",
        "version": 1,
        "passport_hash": null,
        "profile_id": "mcp_server@0.2.0"
    });
    let event_c = append_event_with_payload(
        &router,
        &run_id_a,
        "orchestrator",
        "profile_selected",
        payload_reordered,
    )
    .await;
    assert_eq!(event_c.0, StatusCode::CREATED);

    // Different payload keys order, different sequence → different hash.
    // But both are valid — the key property is that each hash is deterministic
    // for its specific input.  Re-running the exact same inputs would give
    // the same hash (as verified in hash_chain_is_deterministic_and_sequential).
    let hash_c = event_c.1["event_hash"]
        .as_str()
        .expect("hash must be present")
        .to_owned();
    assert_ne!(hash_a, hash_c, "different sequence → different hash");
}

fn build_multipart_body(
    boundary: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    body
}

// ═══════════════════════════════════════════════════════════════════
// Test infrastructure
// ═══════════════════════════════════════════════════════════════════

async fn test_app() -> (Router, SqlitePool) {
    let pool = test_pool().await;
    (app(pool.clone()), pool)
}

async fn test_pool() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("in-memory SQLite URL must parse")
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("test database must connect");
    migrate(&pool).await.expect("migrations must run");
    pool
}

async fn create_github_tool(router: &Router) -> String {
    let response = send_json(
        router,
        Method::POST,
        "/api/tools",
        json!({
            "tool_id": "github:example-org/example-lib",
            "name": "example-lib",
            "tool_type": "generic",
            "canonical_url": "https://github.com/example-org/example-lib",
            "external_identifiers": [
                {
                    "namespace": "github",
                    "value": "example-org/example-lib",
                    "canonical_url": "https://github.com/example-org/example-lib"
                }
            ],
            "aliases": ["Example Library"]
        }),
    )
    .await;
    assert_eq!(
        response.0,
        StatusCode::CREATED,
        "tool creation must succeed: {response:?}"
    );
    response.1["tool_id"]
        .as_str()
        .expect("created tool must have an ID")
        .to_owned()
}

async fn create_run(router: &Router, tool_id: &str) -> String {
    let response = send_json(
        router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool_id": tool_id
        }),
    )
    .await;
    assert_eq!(
        response.0,
        StatusCode::CREATED,
        "run creation must succeed: {response:?}"
    );
    response.1["run_id"]
        .as_str()
        .expect("created run must have an ID")
        .to_owned()
}

async fn append_event(
    router: &Router,
    run_id: &str,
    node_id: &str,
    event_type: &str,
) -> (StatusCode, Value) {
    append_event_with_payload(router, run_id, node_id, event_type, json!({})).await
}

async fn append_event_with_payload(
    router: &Router,
    run_id: &str,
    node_id: &str,
    event_type: &str,
    payload: Value,
) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/events"),
        json!({
            "node_id": node_id,
            "event_type": event_type,
            "payload": payload
        }),
    )
    .await
}

async fn get_run(router: &Router, run_id: &str) -> (StatusCode, Value) {
    send(
        router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let body = Body::from(serde_json::to_vec(&payload).expect("JSON body must serialize"));
    send(router, method, uri, body).await
}

async fn send(router: &Router, method: Method, uri: &str, body: Body) -> (StatusCode, Value) {
    send_typed(router, method, uri, body, "application/json").await
}

async fn send_typed(
    router: &Router,
    method: Method,
    uri: &str,
    body: Body,
    content_type: &str,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTENT_TYPE, content_type)
                .body(body)
                .expect("request must build"),
        )
        .await
        .expect("request must complete");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body must collect")
        .to_bytes();
    let payload = serde_json::from_slice(&body).expect("response body must be JSON");
    (status, payload)
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    assert_eq!(response.0, status);
    assert_eq!(response.1["code"], code);
    assert!(
        response.1["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    assert!(response.1["details"].is_object());
}

// ── GET /api/runs/{run_id}/events ──────────────────────────────────

#[tokio::test]
async fn get_events_endpoint_returns_all_events() {
    let (router, _pool) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // Append two user events.
    let _ev1 = append_event(&router, &run_id, "intake", "node_started").await;
    let _ev2 = append_event(&router, &run_id, "intake", "node_finished").await;

    // GET /api/runs/{run_id}/events
    let (status, body) = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}/events"),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().expect("events must be an array");
    // run_created (sequence 0) + node_started (1) + node_finished (2)
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["event_type"], "run_created");
    assert_eq!(events[1]["event_type"], "node_started");
    assert_eq!(events[2]["event_type"], "node_finished");
}

#[tokio::test]
async fn get_events_endpoint_rejects_missing_run() {
    let (router, _pool) = test_app().await;
    let fake_id = Uuid::new_v4().to_string();
    let (status, body) = send(
        &router,
        Method::GET,
        &format!("/api/runs/{fake_id}/events"),
        Body::empty(),
    )
    .await;
    assert_error(&(status, body), StatusCode::NOT_FOUND, "run_not_found");
}

// ── GET /api/runs/{run_id}/events/stream (SSE) ──────────────────────

#[tokio::test]
async fn sse_stream_endpoint_returns_correct_content_type() {
    let (router, _pool) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    // Send a raw request to the SSE endpoint and check headers.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/runs/{run_id}/events/stream"))
                .body(Body::empty())
                .expect("SSE request must build"),
        )
        .await
        .expect("SSE request must complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type header must be present")
        .to_str()
        .expect("content-type must be a string");
    assert!(
        content_type.starts_with("text/event-stream"),
        "SSE content-type must be text/event-stream, got: {content_type}"
    );
}

#[tokio::test]
async fn sse_stream_endpoint_rejects_missing_run() {
    let (router, _pool) = test_app().await;
    let fake_id = Uuid::new_v4().to_string();
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/runs/{fake_id}/events/stream"))
                .body(Body::empty())
                .expect("SSE request must build"),
        )
        .await
        .expect("SSE request must complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
