use derive_builder::Builder;

use super::{signer::SignerConfig, value::XPub as XPubValue};
use crate::primitives::*;

pub struct AccountXPub {
    pub account_id: AccountId,
    pub key_name: String,
    pub value: XPubValue,
    pub signer: Option<SignerConfig>,
}

impl AccountXPub {
    pub fn id(&self) -> XPubId {
        self.value.id()
    }
}

#[derive(Builder, Clone, Debug)]
pub struct NewXPub {
    pub(super) account_id: AccountId,
    #[builder(setter(into))]
    pub(super) key_name: String,
    pub(super) value: XPubValue,
}

impl NewXPub {
    pub fn builder() -> NewXPubBuilder {
        NewXPubBuilder::default()
    }

    pub fn id(&self) -> XPubId {
        self.value.id()
    }
}
