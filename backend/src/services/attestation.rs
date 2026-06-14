use std::{future::Future, pin::Pin, str::FromStr};

use alloy::{
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use thiserror::Error;

use crate::domain::{AttestationCommitment, AttestationPreflight};

const SEPOLIA_CHAIN_ID: u64 = 11_155_111;

sol! {
    #[sol(rpc)]
    interface ToolPassportRegistry {
        function recordPassport(
            bytes32 runId,
            string toolId,
            string toolType,
            bytes32 passportHash,
            bytes32 auditLogHash,
            bytes32 evidenceManifestHash
        ) external;
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChainSubmission {
    pub transaction_hash: String,
}

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("RPC_URL, PRIVATE_KEY, and REGISTRY_CONTRACT must be configured for attestation")]
    MissingConfiguration,
    #[error("invalid attestation chain configuration: {0}")]
    InvalidConfiguration(String),
    #[error("connected chain ID {actual} does not match approved chain ID {approved}")]
    WrongChain { approved: u64, actual: u64 },
    #[error("attestation transaction failed: {0}")]
    Submission(String),
    #[error("attestation transaction was mined but reverted")]
    Reverted,
}

pub trait AttestationSubmitter: Send + Sync {
    fn preflight(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AttestationPreflight, AttestationError>> + Send>>;

    fn submit(
        &self,
        commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>>;
}

#[derive(Debug, Default)]
pub struct AlloyAttestationSubmitter;

impl AttestationSubmitter for AlloyAttestationSubmitter {
    fn preflight(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AttestationPreflight, AttestationError>> + Send>> {
        Box::pin(async move {
            let (provider, signer, registry_contract) = configured_provider()?;
            let connected_chain_id = provider
                .get_chain_id()
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            let signer_address = signer.address();
            let signer_balance = provider
                .get_balance(signer_address)
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            let registry_address = parse_address("REGISTRY_CONTRACT", &registry_contract)?;
            let registry_code = provider
                .get_code_at(registry_address)
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            let registry_code_present = !registry_code.is_empty();
            let mut issues = Vec::new();
            if connected_chain_id != SEPOLIA_CHAIN_ID {
                issues.push(format!(
                    "connected chain ID {connected_chain_id} is not Sepolia {SEPOLIA_CHAIN_ID}"
                ));
            }
            if signer_balance.is_zero() {
                issues.push("signer has zero balance".to_owned());
            }
            if !registry_code_present {
                issues.push("registry address has no deployed code".to_owned());
            }

            Ok(AttestationPreflight {
                attestation_preflight_schema_version: "0.1.0".to_owned(),
                ready: issues.is_empty(),
                expected_chain_id: SEPOLIA_CHAIN_ID,
                connected_chain_id,
                signer_address: signer_address.to_string(),
                signer_balance_wei: signer_balance.to_string(),
                registry_contract,
                registry_code_present,
                issues,
            })
        })
    }

    fn submit(
        &self,
        commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>> {
        Box::pin(async move {
            let (provider, _signer, configured_registry) = configured_provider()?;
            ensure_registry_matches(&configured_registry, &commitment.registry_contract)?;
            let actual_chain = provider
                .get_chain_id()
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            if actual_chain != commitment.chain_id {
                return Err(AttestationError::WrongChain {
                    approved: commitment.chain_id,
                    actual: actual_chain,
                });
            }

            let address =
                parse_address("approved registry_contract", &commitment.registry_contract)?;
            let contract = ToolPassportRegistry::new(address, provider);
            let pending = contract
                .recordPassport(
                    parse_hash("onchain_run_id", &commitment.onchain_run_id)?,
                    commitment.tool_id,
                    commitment.tool_type,
                    parse_hash("passport_hash", &commitment.passport_hash)?,
                    parse_hash("audit_log_hash", &commitment.audit_log_hash)?,
                    parse_hash("evidence_manifest_hash", &commitment.evidence_manifest_hash)?,
                )
                .send()
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            let transaction_hash = pending.tx_hash().to_string();
            let receipt = pending
                .get_receipt()
                .await
                .map_err(|error| AttestationError::Submission(error.to_string()))?;
            if !receipt.status() {
                return Err(AttestationError::Reverted);
            }
            Ok(ChainSubmission { transaction_hash })
        })
    }
}

fn configured_provider()
-> Result<(impl Provider + Clone, PrivateKeySigner, String), AttestationError> {
    let rpc_url = std::env::var("RPC_URL").map_err(|_| AttestationError::MissingConfiguration)?;
    let private_key =
        std::env::var("PRIVATE_KEY").map_err(|_| AttestationError::MissingConfiguration)?;
    let registry_contract =
        std::env::var("REGISTRY_CONTRACT").map_err(|_| AttestationError::MissingConfiguration)?;
    let signer = PrivateKeySigner::from_str(&private_key)
        .map_err(|error| invalid_configuration("PRIVATE_KEY", error))?;
    let url = rpc_url
        .parse::<alloy::transports::http::reqwest::Url>()
        .map_err(|error| invalid_configuration("RPC_URL", error))?;
    let provider = ProviderBuilder::new()
        .wallet(signer.clone())
        .connect_http(url);
    Ok((provider, signer, registry_contract))
}

fn parse_hash(field: &str, value: &str) -> Result<B256, AttestationError> {
    B256::from_str(value).map_err(|error| invalid_configuration(field, error))
}

fn parse_address(field: &str, value: &str) -> Result<Address, AttestationError> {
    Address::from_str(value).map_err(|error| invalid_configuration(field, error))
}

fn invalid_configuration(field: &str, error: impl std::fmt::Display) -> AttestationError {
    AttestationError::InvalidConfiguration(format!("{field}: {error}"))
}

fn ensure_registry_matches(configured: &str, approved: &str) -> Result<(), AttestationError> {
    if configured.eq_ignore_ascii_case(approved) {
        Ok(())
    } else {
        Err(AttestationError::InvalidConfiguration(
            "approved registry contract does not match REGISTRY_CONTRACT".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{AttestationError, ensure_registry_matches, parse_address};

    #[test]
    fn approved_registry_must_match_runtime_configuration() {
        assert!(
            ensure_registry_matches(
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            )
            .is_ok()
        );
        assert!(matches!(
            ensure_registry_matches(
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            Err(AttestationError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn invalid_registry_error_names_field_without_echoing_value() {
        let invalid_value = "not-an-address";
        let error = parse_address("REGISTRY_CONTRACT", invalid_value).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("REGISTRY_CONTRACT"));
        assert!(!message.contains(invalid_value));
    }
}
