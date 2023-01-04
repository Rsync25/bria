mod constants;
mod templates;

use sqlx::{PgPool, Postgres, Transaction};
use sqlx_ledger::{
    account::NewAccount as NewLedgerAccount, balance::AccountBalance as LedgerAccountBalance,
    journal::*, AccountId as LedgerAccountId, Currency, DebitOrCredit, JournalId, SqlxLedger,
    SqlxLedgerError,
};
use tracing::instrument;
use uuid::Uuid;

use crate::{error::*, primitives::*};
use constants::*;
pub use templates::*;

#[derive(Debug, Clone)]
pub struct Ledger {
    inner: SqlxLedger,
    btc: Currency,
}

#[derive(Debug, Clone)]
pub struct LedgerAccountIdsForWallet {
    pub incoming_id: LedgerAccountId,
    pub at_rest_id: LedgerAccountId,
    pub fee_id: LedgerAccountId,
    pub outgoing_id: LedgerAccountId,
    pub dust_id: LedgerAccountId,
}

impl Ledger {
    pub fn new(pool: &PgPool) -> Self {
        Self {
            inner: SqlxLedger::new(pool),
            btc: "BTC".parse().unwrap(),
        }
    }

    pub async fn init(pool: &PgPool) -> Result<Self, BriaError> {
        let inner = SqlxLedger::new(pool);

        // Create onchain accounts
        Self::onchain_income_account(&inner).await?;
        Self::onchain_at_rest_account(&inner).await?;
        Self::onchain_fee_account(&inner).await?;
        Self::onchain_outgoing_account(&inner).await?;

        templates::IncomingUtxo::init(&inner).await?;
        templates::ConfirmedUtxo::init(&inner).await?;
        templates::QueuedPayout::init(&inner).await?;

        Ok(Self {
            inner,
            btc: "BTC".parse().unwrap(),
        })
    }

    #[instrument(name = "ledger.incoming_utxo", skip(self, tx))]
    pub async fn incoming_utxo(
        &self,
        tx: Transaction<'_, Postgres>,
        params: IncomingUtxoParams,
    ) -> Result<(), BriaError> {
        self.inner
            .post_transaction_in_tx(tx, INCOMING_UTXO_CODE, Some(params))
            .await?;
        Ok(())
    }

    #[instrument(name = "ledger.confirmed_utxo", skip(self, tx))]
    pub async fn confirmed_utxo(
        &self,
        tx: Transaction<'_, Postgres>,
        params: ConfirmedUtxoParams,
    ) -> Result<(), BriaError> {
        self.inner
            .post_transaction_in_tx(tx, CONFIRMED_UTXO_CODE, Some(params))
            .await?;
        Ok(())
    }

    #[instrument(name = "ledger.queued_payout", skip(self, tx))]
    pub async fn queued_payout(
        &self,
        tx: Transaction<'_, Postgres>,
        params: QueuedPayoutParams,
    ) -> Result<(), BriaError> {
        self.inner
            .post_transaction_in_tx(tx, QUEUED_PAYOUT_CODE, Some(params))
            .await?;
        Ok(())
    }

    #[instrument(name = "ledger.get_balance")]
    pub async fn get_balance(
        &self,
        journal_id: JournalId,
        account_id: LedgerAccountId,
    ) -> Result<Option<LedgerAccountBalance>, BriaError> {
        let balance = self
            .inner
            .balances()
            .find(journal_id, account_id, self.btc)
            .await?;
        Ok(balance)
    }

    #[instrument(name = "ledger.create_journal_for_account", skip(self, tx))]
    pub async fn create_journal_for_account(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: AccountId,
        account_name: String,
    ) -> Result<JournalId, BriaError> {
        let new_journal = NewJournal::builder()
            .id(Uuid::from(id))
            .description(format!("Journal for account '{}'", account_name))
            .name(account_name)
            .build()
            .expect("Couldn't build NewJournal");
        let id = self.inner.journals().create_in_tx(tx, new_journal).await?;
        Ok(id)
    }

    #[instrument(name = "ledger.create_ledger_accounts_for_wallet", skip(self, tx))]
    pub async fn create_ledger_accounts_for_wallet(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        wallet_id: WalletId,
        wallet_name: &str,
    ) -> Result<LedgerAccountIdsForWallet, BriaError> {
        let account_ids = LedgerAccountIdsForWallet {
            incoming_id: self
                .create_account_for_wallet(
                    tx,
                    wallet_id,
                    format!("WALLET_{}_INCOMING", wallet_id),
                    format!("{}-incoming", wallet_id),
                )
                .await?,
            at_rest_id: LedgerAccountId::new(),
            fee_id: LedgerAccountId::new(),
            outgoing_id: LedgerAccountId::new(),
            dust_id: self
                .create_account_for_wallet(
                    tx,
                    wallet_id,
                    format!("WALLET_{}_DUST", wallet_id),
                    format!("{}-dust", wallet_id),
                )
                .await?,
        };
        Ok(account_ids)
    }

    #[instrument(name = "ledger.create_account_for_wallet", skip(self, tx))]
    async fn create_account_for_wallet(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        wallet_id: WalletId,
        wallet_code: String,
        wallet_name: String,
    ) -> Result<LedgerAccountId, BriaError> {
        let account = NewLedgerAccount::builder()
            .name(&wallet_name)
            .code(wallet_code)
            .description(format!("Account for wallet '{}'", &wallet_id))
            .build()
            .expect("Couldn't build NewLedgerAccount");
        let account_id = self.inner.accounts().create_in_tx(tx, account).await?;
        Ok(account_id)
    }

    #[instrument(name = "ledger.onchain_income_account", skip_all)]
    async fn onchain_income_account(ledger: &SqlxLedger) -> Result<LedgerAccountId, BriaError> {
        let new_account = NewLedgerAccount::builder()
            .code(ONCHAIN_INCOMING_CODE)
            .id(ONCHAIN_INCOMING_ID)
            .name(ONCHAIN_INCOMING_CODE)
            .description("Account for onchain incoming unconfirmed funds".to_string())
            .normal_balance_type(DebitOrCredit::Debit)
            .build()
            .expect("Couldn't create onchain incoming account");
        match ledger.accounts().create(new_account).await {
            Err(SqlxLedgerError::DuplicateKey(_)) => Ok(LedgerAccountId::from(ONCHAIN_INCOMING_ID)),
            Err(e) => Err(e.into()),
            Ok(id) => Ok(id),
        }
    }

    #[instrument(name = "ledger.onchain_at_rest_account", skip_all)]
    async fn onchain_at_rest_account(ledger: &SqlxLedger) -> Result<LedgerAccountId, BriaError> {
        let new_account = NewLedgerAccount::builder()
            .code(ONCHAIN_AT_REST_CODE)
            .id(ONCHAIN_AT_REST_ID)
            .name(ONCHAIN_AT_REST_CODE)
            .description("Account for settlement of onchain funds".to_string())
            .normal_balance_type(DebitOrCredit::Debit)
            .build()
            .expect("Couldn't create onchain at rest account");
        match ledger.accounts().create(new_account).await {
            Err(SqlxLedgerError::DuplicateKey(_)) => Ok(LedgerAccountId::from(ONCHAIN_AT_REST_ID)),
            Err(e) => Err(e.into()),
            Ok(id) => Ok(id),
        }
    }

    #[instrument(name = "ledger.onchain_fee_account", skip_all)]
    async fn onchain_fee_account(ledger: &SqlxLedger) -> Result<LedgerAccountId, BriaError> {
        let new_account = NewLedgerAccount::builder()
            .code(ONCHAIN_FEE_CODE)
            .id(ONCHAIN_FEE_ID)
            .name(ONCHAIN_FEE_CODE)
            .description("Account for provisioning of onchain fees".to_string())
            .normal_balance_type(DebitOrCredit::Debit)
            .build()
            .expect("Couldn't create onchain fee account");
        match ledger.accounts().create(new_account).await {
            Err(SqlxLedgerError::DuplicateKey(_)) => Ok(LedgerAccountId::from(ONCHAIN_FEE_ID)),
            Err(e) => Err(e.into()),
            Ok(id) => Ok(id),
        }
    }

    #[instrument(name = "ledger.onchain_outgoing_account", skip_all)]
    async fn onchain_outgoing_account(ledger: &SqlxLedger) -> Result<LedgerAccountId, BriaError> {
        let new_account = NewLedgerAccount::builder()
            .code(ONCHAIN_OUTGOING_CODE)
            .id(ONCHAIN_OUTGOING_ID)
            .name(ONCHAIN_OUTGOING_CODE)
            .description("Account for outgoing onchain funds".to_string())
            .normal_balance_type(DebitOrCredit::Debit)
            .build()
            .expect("Couldn't create onchain  account");
        match ledger.accounts().create(new_account).await {
            Err(SqlxLedgerError::DuplicateKey(_)) => Ok(LedgerAccountId::from(ONCHAIN_OUTGOING_ID)),
            Err(e) => Err(e.into()),
            Ok(id) => Ok(id),
        }
    }
}
