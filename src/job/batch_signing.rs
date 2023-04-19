use serde::{Deserialize, Serialize};
use tracing::instrument;

use std::collections::HashMap;

use crate::{
    app::BlockchainConfig, batch::*, error::*, primitives::*, signing_session::*, wallet::*,
    xpub::*,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSigningData {
    pub(super) account_id: AccountId,
    pub(super) batch_id: BatchId,
    #[serde(flatten)]
    pub(super) tracing_data: HashMap<String, String>,
}

#[instrument(
    name = "job.batch_wallet_signing",
    skip(pool, wallets, signing_sessions, batches, xpubs),
    err
)]
pub async fn execute(
    pool: sqlx::PgPool,
    data: BatchSigningData,
    blockchain_cfg: BlockchainConfig,
    batches: Batches,
    signing_sessions: SigningSessions,
    wallets: Wallets,
    xpubs: XPubs,
) -> Result<BatchSigningData, BriaError> {
    let sessions = if let Some(batch_session) = signing_sessions
        .find_for_batch(data.account_id, data.batch_id)
        .await?
    {
        (batch_session.xpub_sessions, HashMap::new())
    } else {
        let mut new_sessions = Vec::new();
        let mut account_xpubs = HashMap::new();
        let batch = batches.find_by_id(data.batch_id).await?;
        let unsigned_psbt = batch.unsigned_psbt;
        for (wallet_id, keychain_utxos) in batch.included_utxos {
            let wallet = wallets.find_by_id(wallet_id).await?;
            let keychain_xpubs = wallet.xpubs_for_keychains(keychain_utxos.keys());
            for (keychain_id, keychain_xpubs) in keychain_xpubs.into_iter() {
                for xpub in keychain_xpubs.into_iter() {
                    let account_xpub = xpubs
                        .find_from_ref(data.account_id, xpub.id().to_string())
                        .await?;
                    let new_session = NewSigningSession::builder()
                        .account_id(data.account_id)
                        .batch_id(data.batch_id)
                        .wallet_id(wallet_id)
                        .keychain_id(keychain_id)
                        .xpub(xpub)
                        .unsigned_psbt(unsigned_psbt.clone())
                        .build()
                        .expect("Could not build signing session");
                    new_sessions.push(new_session);
                    account_xpubs.insert(account_xpub.id(), account_xpub);
                }
            }
        }

        (HashMap::new(), account_xpubs)
    };

    // let wallet = wallets.find_by_id(data.wallet_id).await?;
    // if let Some(keychain_utxos) = batch.included_utxos.get(&data.wallet_id) {
    //     let keychain_xpubs = wallet.xpubs_for_keychains(keychain_utxos.keys());
    //     for (keychain_id, keychain_xpubs) in keychain_xpubs.into_iter() {
    //         for xpub in keychain_xpubs.into_iter() {
    //             let account_xpub = xpubs.find_from_ref(data.account_id, xpub.id().to_string());
    //             let new_session = NewSigningSession::builder()
    //                 .account_id(data.account_id)
    //                 .batch_id(data.batch_id)
    //                 .xpub(xpub)
    //                 .build()
    //                 .expect("Could not build signing session");
    //         }
    //     }
    // }
    // let wallet.xpubs_for_keychains
    // load and sign psbt
    // for each spent utxo
    // for each keychain_id => fetch all xpubs
    // => for each xpub fetch signing config
    // => sign psbt
    // => persist signed psbt
    Ok(data)
}