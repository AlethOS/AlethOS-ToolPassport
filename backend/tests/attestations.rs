use std::{
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use toolpassport_backend::{
    AttestationCommitment, AttestationError, AttestationPreflight, AttestationSubmitter,
    ChainSubmission, StorageService, app_with_storage_and_submitter, migrate,
};
use tower::ServiceExt;
use uuid::Uuid;

const HASH_1: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const HASH_2: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";
const HASH_3: &str = "0x3333333333333333333333333333333333333333333333333333333333333333";
const HASH_4: &str = "0x4444444444444444444444444444444444444444444444444444444444444444";
const TX_HASH: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CONTRACT: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(Default)]
struct RecordingSubmitter {
    commitments: Mutex<Vec<AttestationCommitment>>,
}

#[derive(Default)]
struct FailingSubmitter {
    calls: Mutex<u64>,
}

impl AttestationSubmitter for FailingSubmitter {
    fn preflight(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AttestationPreflight, AttestationError>> + Send>> {
        Box::pin(async { Err(AttestationError::MissingConfiguration) })
    }

    fn submit(
        &self,
        _commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>> {
        *self.calls.lock().expect("call lock") += 1;
        Box::pin(async { Err(AttestationError::Submission("RPC unavailable".to_owned())) })
    }
}

impl AttestationSubmitter for RecordingSubmitter {
    fn preflight(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AttestationPreflight, AttestationError>> + Send>> {
        Box::pin(async {
            Ok(AttestationPreflight {
                attestation_preflight_schema_version: "0.1.0".to_owned(),
                ready: true,
                expected_chain_id: 11_155_111,
                connected_chain_id: 11_155_111,
                signer_address: "0xcccccccccccccccccccccccccccccccccccccccc".to_owned(),
                signer_balance_wei: "1000000000000000".to_owned(),
                registry_contract: CONTRACT.to_owned(),
                registry_code_present: true,
                issues: vec![],
            })
        })
    }

    fn submit(
        &self,
        commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>> {
        self.commitments
            .lock()
            .expect("commitment lock")
            .push(commitment);
        Box::pin(async {
            Ok(ChainSubmission {
                transaction_hash: TX_HASH.to_owned(),
            })
        })
    }
}

#[tokio::test]
async fn exposes_only_public_attestation_preflight_values() {
    let pool = test_pool().await;
    let router = test_app(pool, Arc::new(RecordingSubmitter::default()));

    let response = send(&router, Method::GET, "/api/attestation/preflight").await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["ready"], true);
    assert_eq!(response.1["connected_chain_id"], 11_155_111);
    assert_eq!(response.1["registry_contract"], CONTRACT);
    assert!(response.1.get("rpc_url").is_none());
    assert!(response.1.get("private_key").is_none());
}

#[tokio::test]
async fn submits_exact_approved_commitment_and_persists_immutable_receipt() {
    let pool = test_pool().await;
    let submitter = Arc::new(RecordingSubmitter::default());
    let router = test_app(pool.clone(), submitter.clone());
    let run_id = seed_attestation_run(&pool, "approve_testnet_attestation", "running").await;

    let submitted = send(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(submitted.0, StatusCode::CREATED);
    assert_eq!(submitted.1["status"], "confirmed");
    assert_eq!(submitted.1["transaction_hash"], TX_HASH);
    assert_eq!(submitted.1["registry_contract"], CONTRACT);
    assert_eq!(submitted.1["passport_hash"], HASH_1);

    let commitments = submitter.commitments.lock().expect("commitment lock");
    assert_eq!(commitments.len(), 1);
    assert_eq!(
        commitments[0],
        AttestationCommitment {
            run_id,
            tool_id: "github:example/tool".to_owned(),
            tool_type: "generic".to_owned(),
            passport_hash: HASH_1.to_owned(),
            audit_log_hash: HASH_2.to_owned(),
            evidence_manifest_hash: HASH_3.to_owned(),
            onchain_run_id: HASH_4.to_owned(),
            chain_id: 11_155_111,
            registry_contract: CONTRACT.to_owned(),
        }
    );
    drop(commitments);

    let details = send(&router, Method::GET, &format!("/api/runs/{run_id}")).await;
    assert_eq!(details.1["run"]["status"], "success");
    assert_eq!(details.1["run"]["current_node"], "attest_onchain");
    let events = details.1["events"].as_array().expect("events");
    assert_eq!(events[1]["event_type"], "attestation_submitted");
    assert_eq!(events[2]["event_type"], "attestation_confirmed");

    let fetched = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(fetched.0, StatusCode::OK);
    assert_eq!(fetched.1["transaction_hash"], TX_HASH);

    let duplicate = send(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::CONFLICT);
    assert_eq!(
        submitter.commitments.lock().expect("commitment lock").len(),
        1
    );

    let update_error =
        sqlx::query("UPDATE attestation_receipts SET confirmed_at = NULL WHERE run_id = ?")
            .bind(run_id.to_string())
            .execute(&pool)
            .await
            .expect_err("receipt update must be rejected");
    assert!(update_error.to_string().contains("immutable"));
}

#[tokio::test]
async fn refuses_submission_without_testnet_approval_before_calling_submitter() {
    let pool = test_pool().await;
    let submitter = Arc::new(RecordingSubmitter::default());
    let router = test_app(pool.clone(), submitter.clone());
    let run_id = seed_attestation_run(&pool, "approve_offchain", "running").await;

    let response = send(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(response.0, StatusCode::CONFLICT);
    assert!(
        submitter
            .commitments
            .lock()
            .expect("commitment lock")
            .is_empty()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attestation_receipts")
        .fetch_one(&pool)
        .await
        .expect("receipt count");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn never_auto_retries_after_a_submission_attempt_fails() {
    let pool = test_pool().await;
    let submitter = Arc::new(FailingSubmitter::default());
    let router = test_app(pool.clone(), submitter.clone());
    let run_id = seed_attestation_run(&pool, "approve_testnet_attestation", "running").await;

    let failed = send(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(failed.0, StatusCode::BAD_GATEWAY);

    let retry = send(
        &router,
        Method::POST,
        &format!("/api/runs/{run_id}/attestation"),
    )
    .await;
    assert_eq!(retry.0, StatusCode::CONFLICT);
    assert_eq!(retry.1["code"], "attestation_attempt_already_exists");
    assert_eq!(*submitter.calls.lock().expect("call lock"), 1);
}

fn test_app(pool: SqlitePool, submitter: Arc<dyn AttestationSubmitter>) -> Router {
    app_with_storage_and_submitter(
        pool,
        StorageService::new(
            std::env::temp_dir().join(format!("toolpassport-attestation-{}", Uuid::new_v4())),
            1024 * 1024,
        ),
        submitter,
    )
}

async fn test_pool() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("valid SQLite URL")
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("test database");
    migrate(&pool).await.expect("migrations");
    pool
}

async fn seed_attestation_run(pool: &SqlitePool, decision: &str, status: &str) -> Uuid {
    let run_id = Uuid::new_v4();
    let now = "2026-06-14T12:00:00.000Z";
    sqlx::query(
        r#"
        INSERT INTO runs (
            run_id, goal, tool_id, canonical_url, tool_name, tool_type, tool_urls,
            standard_id, standard_version, profile_id, profile_version, status,
            current_node, created_at, updated_at
        ) VALUES (?, 'Audit tool', 'github:example/tool', 'https://github.com/example/tool',
                  'Example Tool', 'generic', '["https://github.com/example/tool"]',
                  'alethos-tool-audit', '0.3.0', 'generic', '0.3.0', ?,
                  'human_review_gate', ?, ?)
        "#,
    )
    .bind(run_id.to_string())
    .bind(status)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed run");
    sqlx::query(
        r#"
        INSERT INTO run_events (
            event_id, run_id, sequence, node_id, event_type, payload, created_at,
            event_hash, prev_event_hash
        ) VALUES (?, ?, 1, 'run', 'run_created', '{"status":"pending"}', ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(run_id.to_string())
    .bind(now)
    .bind(HASH_2)
    .bind("0x0000000000000000000000000000000000000000000000000000000000000000")
    .execute(pool)
    .await
    .expect("seed event");

    let passport = json!({
        "passport_version": "0.2.0",
        "passport_sequence": 1,
        "tool_id": "github:example/tool",
        "run_id": run_id,
        "tool_type": "generic",
        "target_revision": null,
        "audit_scope": "public sources",
        "standard_id": "alethos-tool-audit",
        "standard_version": "0.3.0",
        "profile_id": "generic",
        "profile_version": "0.3.0",
        "evidence_board_version": 1,
        "check_results_id": Uuid::new_v4(),
        "capability_claims": [],
        "interfaces": [],
        "risks": [],
        "known_gaps": [],
        "scores": {
            "dimensions": {
                "capability_clarity": 0, "interface_openness": 0, "automation_readiness": 0,
                "data_portability": 0, "permission_risk": 0, "evidence_quality": 0, "ecosystem_fit": 0
            },
            "total_score": 0,
            "rating": "not_recommended"
        },
        "recommendation": {"summary": "Do not use", "conditions": []}
    });
    let provenance = json!({
        "provenance_schema_version": "0.1.0",
        "run_id": run_id,
        "freeze_version": 1,
        "evidence_board_version": 1,
        "passport_sequence": 1,
        "passport_hash": HASH_1,
        "audit_log_hash": HASH_2,
        "evidence_manifest_hash": HASH_3,
        "onchain_run_id": HASH_4,
        "frozen_at": now
    });
    sqlx::query(
        "INSERT INTO passports (run_id, sequence, passport_json, frozen_at) VALUES (?, 1, ?, ?)",
    )
    .bind(run_id.to_string())
    .bind(passport.to_string())
    .bind(now)
    .execute(pool)
    .await
    .expect("seed passport");
    sqlx::query(
        "INSERT INTO provenances (run_id, freeze_version, provenance_json) VALUES (?, 1, ?)",
    )
    .bind(run_id.to_string())
    .bind(provenance.to_string())
    .execute(pool)
    .await
    .expect("seed provenance");
    let approval_id = Uuid::new_v4();
    let approval = json!({
        "approval_schema_version": "0.1.0",
        "approval_id": approval_id,
        "run_id": run_id,
        "decision": decision,
        "passport_sequence": 1,
        "passport_hash": HASH_1,
        "audit_log_hash": HASH_2,
        "evidence_manifest_hash": HASH_3,
        "chain_id": if decision == "approve_testnet_attestation" { Some(11_155_111_u64) } else { None },
        "registry_contract": if decision == "approve_testnet_attestation" { Some(CONTRACT) } else { None },
        "decided_at": now
    });
    sqlx::query("INSERT INTO approvals (approval_id, run_id, approval_json, decided_at) VALUES (?, ?, ?, ?)")
        .bind(approval_id.to_string())
        .bind(run_id.to_string())
        .bind(approval.to_string())
        .bind(now)
        .execute(pool)
        .await
        .expect("seed approval");
    run_id
}

async fn send(router: &Router, method: Method, path: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("JSON body"))
}
