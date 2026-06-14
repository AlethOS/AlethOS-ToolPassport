use std::{future::Future, pin::Pin, str::FromStr};

use alloy::{
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use thiserror::Error;

use crate::domain::AttestationCommitment;

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
    #[error("RPC_URL and PRIVATE_KEY must be configured before submitting an attestation")]
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
    fn submit(
        &self,
        commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>>;
}

#[derive(Debug, Default)]
pub struct AlloyAttestationSubmitter;

impl AttestationSubmitter for AlloyAttestationSubmitter {
    fn submit(
        &self,
        commitment: AttestationCommitment,
    ) -> Pin<Box<dyn Future<Output = Result<ChainSubmission, AttestationError>> + Send>> {
        Box::pin(async move {
            let rpc_url =
                std::env::var("RPC_URL").map_err(|_| AttestationError::MissingConfiguration)?;
            let private_key =
                std::env::var("PRIVATE_KEY").map_err(|_| AttestationError::MissingConfiguration)?;
            let signer = PrivateKeySigner::from_str(&private_key)
                .map_err(|error| AttestationError::InvalidConfiguration(error.to_string()))?;
            let url = rpc_url
                .parse::<alloy::transports::http::reqwest::Url>()
                .map_err(|error| AttestationError::InvalidConfiguration(error.to_string()))?;
            let provider = ProviderBuilder::new().wallet(signer).connect_http(url);
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

            let address = Address::from_str(&commitment.registry_contract)
                .map_err(|error| AttestationError::InvalidConfiguration(error.to_string()))?;
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

fn parse_hash(field: &str, value: &str) -> Result<B256, AttestationError> {
    B256::from_str(value)
        .map_err(|error| AttestationError::InvalidConfiguration(format!("{field}: {error}")))
}
