use sqlx::{Pool, Postgres, QueryBuilder, Row, Transaction};
use uuid::Uuid;

use std::collections::HashMap;

use super::entity::*;
use crate::{
    error::*,
    primitives::{bitcoin::*, *},
};

pub struct ReservableUtxo {
    pub keychain_id: KeychainId,
    pub income_address: bool,
    pub outpoint: OutPoint,
    pub spending_batch_id: Option<BatchId>,
    pub confirmed_ledger_tx_id: Option<LedgerTransactionId>,
}

#[derive(Clone)]
pub(super) struct UtxoRepo {
    pool: Pool<Postgres>,
}

impl UtxoRepo {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn persist_utxo(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        utxo: NewUtxo,
    ) -> Result<Option<LedgerTransactionId>, BriaError> {
        let result = sqlx::query!(
            r#"INSERT INTO bria_utxos
               (wallet_id, keychain_id, tx_id, vout, sats_per_vbyte_when_created, kind, address_idx, value, address, script_hex, spent, pending_ledger_tx_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
               ON CONFLICT (keychain_id, tx_id, vout) DO NOTHING"#,
          Uuid::from(utxo.wallet_id),
          Uuid::from(utxo.keychain_id),
          utxo.outpoint.txid.to_string(),
          utxo.outpoint.vout as i32,
          utxo.sats_per_vbyte_when_created,
          pg::PgKeychainKind::from(utxo.kind) as pg::PgKeychainKind,
          utxo.address_idx as i32,
          utxo.value.into_inner(),
          utxo.address.to_string(),
          utxo.script_hex,
          utxo.spent,
          Uuid::from(utxo.income_pending_ledger_tx_id)
        )
        .execute(&mut *tx)
        .await?;

        Ok(if result.rows_affected() > 0 {
            Some(utxo.income_pending_ledger_tx_id)
        } else {
            None
        })
    }

    pub async fn mark_utxo_confirmed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        keychain_id: KeychainId,
        outpoint: OutPoint,
        spent: bool,
        block_height: u32,
    ) -> Result<ConfirmedUtxo, BriaError> {
        let new_confirmed_ledger_tx_id = LedgerTransactionId::new();

        let row = sqlx::query!(
            r#"UPDATE bria_utxos
            SET spent = $1,
                block_height = $2,
                confirmed_ledger_tx_id = $3,
                modified_at = NOW()
            WHERE keychain_id = $4
              AND tx_id = $5
              AND vout = $6
            RETURNING address_idx, value, address, pending_ledger_tx_id, spending_batch_id"#,
            spent,
            block_height as i32,
            Uuid::from(new_confirmed_ledger_tx_id),
            Uuid::from(keychain_id),
            outpoint.txid.to_string(),
            outpoint.vout as i32,
        )
        .fetch_one(&mut *tx)
        .await?;

        Ok(ConfirmedUtxo {
            keychain_id,
            address_idx: row.address_idx as u32,
            value: Satoshis::from(row.value),
            address: row.address.parse().expect("couldn't parse address"),
            block_height,
            pending_ledger_tx_id: LedgerTransactionId::from(row.pending_ledger_tx_id),
            confirmed_ledger_tx_id: new_confirmed_ledger_tx_id,
            spending_batch_id: row.spending_batch_id.map(BatchId::from),
        })
    }

    pub async fn find_keychain_utxos(
        &self,
        keychain_ids: impl Iterator<Item = KeychainId>,
    ) -> Result<HashMap<KeychainId, KeychainUtxos>, BriaError> {
        let keychain_ids: Vec<Uuid> = keychain_ids.map(Uuid::from).collect();
        let rows = sqlx::query!(
            r#"SELECT wallet_id, keychain_id, tx_id, vout, kind as "kind: pg::PgKeychainKind", address_idx, value, address, spent as spent,
                  CASE
                      WHEN kind = 'external' THEN address
                      ELSE NULL
                  END as optional_address,
                  block_height, pending_ledger_tx_id, confirmed_ledger_tx_id, spending_batch_id
           FROM bria_utxos
           WHERE keychain_id = ANY($1) AND spent = false
           ORDER BY created_at DESC"#,
           &keychain_ids
        )
            .fetch_all(&self.pool)
            .await?;

        let mut utxos = HashMap::new();

        for row in rows {
            let utxo = WalletUtxo {
                wallet_id: row.wallet_id.into(),
                keychain_id: row.keychain_id.into(),
                outpoint: OutPoint {
                    txid: row.tx_id.parse().unwrap(),
                    vout: row.vout as u32,
                },
                kind: KeychainKind::from(row.kind),
                address_idx: row.address_idx as u32,
                value: Satoshis::from(row.value),
                address: row
                    .optional_address
                    .map(|addr| addr.parse().expect("couldn't parse address")),
                spent: row.spent,
                block_height: row.block_height.map(|v| v as u32),
                pending_ledger_tx_id: LedgerTransactionId::from(row.pending_ledger_tx_id),
                confirmed_ledger_tx_id: row.confirmed_ledger_tx_id.map(LedgerTransactionId::from),
                spending_batch_id: row.spending_batch_id.map(BatchId::from),
            };

            let keychain_id = KeychainId::from(row.keychain_id);
            utxos
                .entry(keychain_id)
                .or_insert_with(|| KeychainUtxos {
                    keychain_id,
                    utxos: Vec::new(),
                })
                .utxos
                .push(utxo);
        }

        Ok(utxos)
    }

    pub async fn find_reservable_utxos(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ids: impl Iterator<Item = KeychainId>,
    ) -> Result<Vec<ReservableUtxo>, BriaError> {
        let uuids = ids.into_iter().map(Uuid::from).collect::<Vec<_>>();
        let rows = sqlx::query!(
            r#"SELECT keychain_id,
               CASE WHEN kind = 'external' THEN true ELSE false END as income_address,
               tx_id, vout, spending_batch_id, confirmed_ledger_tx_id
               FROM bria_utxos
               WHERE keychain_id = ANY($1) AND spent = false
               FOR UPDATE"#,
            &uuids[..]
        )
        .fetch_all(&mut *tx)
        .await?;

        let reservable_utxos = rows
            .into_iter()
            .map(|row| ReservableUtxo {
                keychain_id: KeychainId::from(row.keychain_id),
                income_address: row.income_address.unwrap_or_default(),
                outpoint: OutPoint {
                    txid: row.tx_id.parse().unwrap(),
                    vout: row.vout as u32,
                },
                spending_batch_id: row.spending_batch_id.map(BatchId::from),
                confirmed_ledger_tx_id: row.confirmed_ledger_tx_id.map(LedgerTransactionId::from),
            })
            .collect();

        Ok(reservable_utxos)
    }

    pub async fn reserve_utxos_in_batch(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        batch_id: BatchId,
        utxos: impl Iterator<Item = (KeychainId, OutPoint)>,
    ) -> Result<(), BriaError> {
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"UPDATE bria_utxos
            SET spending_batch_id = "#,
        );
        query_builder.push_bind(Uuid::from(batch_id));
        query_builder.push("WHERE (keychain_id, tx_id, vout) IN");
        query_builder.push_tuples(
            utxos.map(|(keychain_id, utxo)| {
                (
                    Uuid::from(keychain_id),
                    utxo.txid.to_string(),
                    utxo.vout as i32,
                )
            }),
            |mut builder, (keychain_id, tx_id, vout)| {
                builder.push_bind(keychain_id);
                builder.push_bind(tx_id);
                builder.push_bind(vout);
            },
        );

        let query = query_builder.build();
        query.execute(&mut *tx).await?;
        Ok(())
    }

    pub async fn get_pending_ledger_tx_ids_for_utxos(
        &self,
        utxos: &HashMap<KeychainId, Vec<OutPoint>>,
    ) -> Result<Vec<LedgerTransactionId>, BriaError> {
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"SELECT pending_ledger_tx_id
            FROM bria_utxos
            WHERE pending_ledger_tx_id IS NOT NULL AND (keychain_id, tx_id, vout) IN"#,
        );

        query_builder.push_tuples(
            utxos.iter().flat_map(|(keychain_id, utxos)| {
                utxos.iter().map(move |utxo| {
                    (
                        Uuid::from(*keychain_id),
                        utxo.txid.to_string(),
                        utxo.vout as i32,
                    )
                })
            }),
            |mut builder, (keychain_id, tx_id, vout)| {
                builder.push_bind(keychain_id);
                builder.push_bind(tx_id);
                builder.push_bind(vout);
            },
        );

        let query = query_builder.build();
        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|row| LedgerTransactionId::from(row.get::<Uuid, _>("pending_ledger_tx_id")))
            .collect())
    }
}