use thiserror::Error;

use crate::{
    job::JobExecutionError,
    primitives::bitcoin::{bip32, consensus, psbt, AddressError},
    signing_session::SigningFailureReason,
};

#[derive(Error, Debug)]
pub enum BriaError {
    #[error("BriaError - Tonic: {0}")]
    Tonic(#[from] tonic::transport::Error),
    #[error("BriaError - Sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("BriaError - Migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("BriaError - ParseId: {0}")]
    ParseId(#[from] uuid::Error),
    #[error("BriaError - SqlxLedger: {0}")]
    SqlxLedger(#[from] sqlx_ledger::SqlxLedgerError),
    #[error("BriaError - SerdeJson: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("BriaError - ElectrumClient: {0}")]
    ElectrumClient(#[from] electrum_client::Error),
    #[error("BriaError - Bip32: {0}")]
    Bip32(#[from] bip32::Error),
    #[error("BriaError - WalletNotFound")]
    WalletNotFound,
    #[error("BriaError - ProfileNotFound")]
    ProfileNotFound,
    #[error("BriaError - CouldNotRetrieveWalletBalance")]
    CouldNotRetrieveWalletBalance,
    #[error("BriaError - BatchGroupNotFound")]
    BatchGroupNotFound,
    #[error("BriaError - BatchNotFound")]
    BatchNotFound,
    #[error("BriaError - BitcoinConsensusEncodeError: {0}")]
    BitcoinConsensusEncodeError(#[from] consensus::encode::Error),
    #[error("BriaError - TryFromIntError")]
    TryFromIntError(#[from] std::num::TryFromIntError),
    #[error("BriaError - BitcoinAddressParseError")]
    BitcoinAddressParseError(#[from] AddressError),
    #[error("BriaError - XPubDepthMismatch: expected depth {0}, got {1}")]
    XPubDepthMismatch(u8, usize),
    #[error("BriaError - XPubParseError: {0}")]
    XPubParseError(bdk::bitcoin::util::base58::Error),
    #[error("BriaError - JoinError: {0}")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("BriaError - BdkError: {0}")]
    BdkError(#[from] bdk::Error),
    #[error("BriaError - BdkMiniscriptError: {0}")]
    BdkMiniscriptError(#[from] bdk::miniscript::Error),
    #[error("BriaError - FeeEstimation: {0}")]
    FeeEstimation(reqwest::Error),
    #[error("BriaError - CouldNotCombinePsbts: {0}")]
    CouldNotCombinePsbts(psbt::Error),
    #[error("BriaError - CouldNotParseIncomingMetadata: {0}")]
    CouldNotParseIncomingMetadata(serde_json::Error),
    #[error("BriaError - SigningSessionStalled: {0}")]
    SigningSessionStalled(SigningFailureReason),
}

impl JobExecutionError for BriaError {}
