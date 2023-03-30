use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx_ledger::{tx_template::*, JournalId, SqlxLedger, SqlxLedgerError};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    error::*, ledger::constants::*, primitives::*, wallet::balance::WalletLedgerAccountIds,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBatchMeta {
    pub batch_id: BatchId,
    pub batch_group_id: BatchGroupId,
    pub bitcoin_tx_id: bitcoin::Txid,
}

#[derive(Debug)]
pub struct CreateBatchParams {
    pub journal_id: JournalId,
    pub ledger_account_ids: WalletLedgerAccountIds,
    pub total_in_sats: Satoshis,
    pub total_spent_sats: Satoshis,
    pub fee_sats: Satoshis,
    pub reserved_fees: Satoshis,
    pub correlation_id: Uuid,
    pub meta: CreateBatchMeta,
}

impl CreateBatchParams {
    pub fn defs() -> Vec<ParamDefinition> {
        vec![
            ParamDefinition::builder()
                .name("journal_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("logical_outgoing_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("logical_at_rest_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("onchain_fee_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("onchain_at_rest_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("onchain_income_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("onchain_outgoing_account_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("total_in")
                .r#type(ParamDataType::DECIMAL)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("total_spent")
                .r#type(ParamDataType::DECIMAL)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("fees")
                .r#type(ParamDataType::DECIMAL)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("reserved_fees")
                .r#type(ParamDataType::DECIMAL)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("correlation_id")
                .r#type(ParamDataType::UUID)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("meta")
                .r#type(ParamDataType::JSON)
                .build()
                .unwrap(),
            ParamDefinition::builder()
                .name("effective")
                .r#type(ParamDataType::DATE)
                .build()
                .unwrap(),
        ]
    }
}

impl From<CreateBatchParams> for TxParams {
    fn from(
        CreateBatchParams {
            journal_id,
            ledger_account_ids,
            total_in_sats,
            total_spent_sats,
            fee_sats,
            reserved_fees,
            correlation_id,
            meta,
        }: CreateBatchParams,
    ) -> Self {
        let total_in = total_in_sats.to_btc();
        let total_spent = total_spent_sats.to_btc();
        let fee_sats = fee_sats.to_btc();
        let reserved_fees = reserved_fees.to_btc();
        let effective = Utc::now().date_naive();
        let meta = serde_json::to_value(meta).expect("Couldn't serialize meta");
        let mut params = Self::default();
        params.insert("journal_id", journal_id);
        params.insert(
            "logical_outgoing_account_id",
            ledger_account_ids.logical_outgoing_id,
        );
        params.insert(
            "logical_at_rest_account_id",
            ledger_account_ids.logical_at_rest_id,
        );
        params.insert("onchain_fee_account_id", ledger_account_ids.fee_id);
        params.insert(
            "onchain_outgoing_account_id",
            ledger_account_ids.onchain_outgoing_id,
        );
        params.insert(
            "onchain_income_account_id",
            ledger_account_ids.onchain_incoming_id,
        );
        params.insert(
            "onchain_at_rest_account_id",
            ledger_account_ids.onchain_at_rest_id,
        );
        params.insert("total_in", total_in);
        params.insert("total_spent", total_spent);
        params.insert("fees", fee_sats);
        params.insert("reserved_fees", reserved_fees);
        params.insert("correlation_id", correlation_id);
        params.insert("meta", meta);
        params.insert("effective", effective);
        params
    }
}

pub struct CreateBatch {}

impl CreateBatch {
    #[instrument(name = "ledger.create_batch.init", skip_all)]
    pub async fn init(ledger: &SqlxLedger) -> Result<(), BriaError> {
        let tx_input = TxInput::builder()
            .journal_id("params.journal_id")
            .effective("params.effective")
            .correlation_id("params.correlation_id")
            .metadata("params.meta")
            .description("'Construct Batch'")
            .build()
            .expect("Couldn't build TxInput");
        let entries = vec![
            // LOGICAL
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_ENCUMBERED_DR'")
                .currency("'BTC'")
                .account_id("params.logical_outgoing_account_id")
                .direction("DEBIT")
                .layer("ENCUMBERED")
                .units("params.total_spent")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_ENCUMBERED_CR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{LOGICAL_OUTGOING_ID}')"))
                .direction("CREDIT")
                .layer("ENCUMBERED")
                .units("params.total_spent")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_PENDING_CR'")
                .currency("'BTC'")
                .account_id("params.logical_outgoing_account_id")
                .direction("CREDIT")
                .layer("PENDING")
                .units("params.total_spent")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_PENDING_DR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{LOGICAL_OUTGOING_ID}')"))
                .direction("DEBIT")
                .layer("PENDING")
                .units("params.total_spent")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_SETTLED_DR'")
                .currency("'BTC'")
                .account_id("params.logical_at_rest_account_id")
                .direction("DEBIT")
                .layer("SETTLED")
                .units("params.total_spent + params.fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_LOGICAL_SETTLED_CR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{LOGICAL_AT_REST_ID}')"))
                .direction("CREDIT")
                .layer("SETTLED")
                .units("params.total_spent + params.fees")
                .build()
                .expect("Couldn't build entry"),
            // FEES
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_FEE_PENDING_DR'")
                .currency("'BTC'")
                .account_id("params.onchain_fee_account_id")
                .direction("DEBIT")
                .layer("PENDING")
                .units("params.fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_FEE_PENDING_CR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{ONCHAIN_FEE_ID}')"))
                .direction("CREDIT")
                .layer("PENDING")
                .units("params.fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_FEE_ENCUMBERED_CR'")
                .currency("'BTC'")
                .account_id("params.onchain_fee_account_id")
                .direction("CREDIT")
                .layer("ENCUMBERED")
                .units("params.reserved_fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_FEE_ENCUMBERED_DR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{ONCHAIN_FEE_ID}')"))
                .direction("DEBIT")
                .layer("ENCUMBERED")
                .units("params.reserved_fees")
                .build()
                .expect("Couldn't build entry"),
            // UTXO
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_PENDING_DR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{ONCHAIN_UTXO_OUTGOING_ID}')"))
                .direction("DEBIT")
                .layer("PENDING")
                .units("params.total_in - params.fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_PENDING_CR'")
                .currency("'BTC'")
                .account_id("params.onchain_outgoing_account_id")
                .direction("CREDIT")
                .layer("PENDING")
                .units("params.total_in - params.fees")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_SETTLED_DR'")
                .currency("'BTC'")
                .account_id("params.onchain_at_rest_account_id")
                .direction("DEBIT")
                .layer("SETTLED")
                .units("params.total_in")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_SETTLED_CR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{ONCHAIN_UTXO_AT_REST_ID}')"))
                .direction("CREDIT")
                .layer("SETTLED")
                .units("params.total_in")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_ENCUMBERED_DR'")
                .currency("'BTC'")
                .account_id(format!("uuid('{ONCHAIN_UTXO_INCOMING_ID}')"))
                .direction("DEBIT")
                .layer("ENCUMBERED")
                .units("params.total_in - params.fees - params.total_spent")
                .build()
                .expect("Couldn't build entry"),
            EntryInput::builder()
                .entry_type("'CREATE_BATCH_UTXO_ENCUMBERED_CR'")
                .currency("'BTC'")
                .account_id("params.onchain_income_account_id")
                .direction("CREDIT")
                .layer("ENCUMBERED")
                .units("params.total_in - params.fees - params.total_spent")
                .build()
                .expect("Couldn't build entry"),
        ];

        let params = CreateBatchParams::defs();
        let template = NewTxTemplate::builder()
            .id(CREATE_BATCH_ID)
            .code(CREATE_BATCH_CODE)
            .tx_input(tx_input)
            .entries(entries)
            .params(params)
            .build()
            .expect("Couldn't build CREATE_BATCH_CODE");
        match ledger.tx_templates().create(template).await {
            Err(SqlxLedgerError::DuplicateKey(_)) => Ok(()),
            Err(e) => Err(e.into()),
            Ok(_) => Ok(()),
        }
    }
}
