use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use super::{signing_client::*, value::XPub as XPubValue};
use crate::{entity::*, primitives::*};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignerConfig {
    Lnd(LndSignerConfig),
    Bitcoind(BitcoindSignerConfig),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum XPubEvent {
    // Spelling is Xpub for nicer serialization (not x_pub_initialized)
    XpubInitialized {
        db_uuid: uuid::Uuid,
        account_id: AccountId,
        fingerprint: bitcoin::Fingerprint,
        parent_fingerprint: bitcoin::Fingerprint,
        original: String,
        xpub: bitcoin::ExtendedPubKey,
        derivation_path: Option<bitcoin::DerivationPath>,
    },
    XpubNameUpdated {
        name: String,
    },
    SignerConfigUpdated {
        config: SignerConfig,
    },
}

#[derive(Builder)]
#[builder(pattern = "owned", build_fn(error = "EntityError"))]
pub struct AccountXPub {
    pub account_id: AccountId,
    pub key_name: String,
    pub value: XPubValue,
    pub(super) db_uuid: uuid::Uuid,
    pub(super) events: EntityEvents<XPubEvent>,
}

impl AccountXPub {
    pub fn id(&self) -> XPubId {
        self.value.id()
    }

    pub fn set_signer_config(&mut self, config: SignerConfig) {
        self.events.push(XPubEvent::SignerConfigUpdated { config })
    }

    fn signing_cfg(&self) -> Option<&SignerConfig> {
        let mut ret = None;
        for event in self.events.iter() {
            if let XPubEvent::SignerConfigUpdated { config } = event {
                ret = Some(config)
            }
        }
        ret
    }

    pub async fn remote_signing_client(
        &self,
    ) -> Result<Option<Box<dyn RemoteSigningClient + 'static>>, SigningClientError> {
        let client = match self.signing_cfg() {
            Some(SignerConfig::Lnd(ref cfg)) => {
                let client = LndRemoteSigner::connect(cfg).await?;
                Some(Box::new(client) as Box<dyn RemoteSigningClient + 'static>)
            }
            Some(SignerConfig::Bitcoind(ref cfg)) => {
                let client = BitcoindRemoteSigner::connect(cfg).await?;
                Some(Box::new(client) as Box<dyn RemoteSigningClient + 'static>)
            }
            None => None,
        };
        Ok(client)
    }
}

#[derive(Builder, Clone, Debug)]
pub struct NewAccountXPub {
    pub(super) db_uuid: uuid::Uuid,
    pub(super) account_id: AccountId,
    #[builder(setter(into))]
    pub(super) key_name: String,
    pub(super) original: String,
    pub(super) value: XPubValue,
}

impl NewAccountXPub {
    pub fn builder() -> NewAccountXPubBuilder {
        let mut builder = NewAccountXPubBuilder::default();
        builder.db_uuid(uuid::Uuid::new_v4());
        builder
    }

    pub fn id(&self) -> XPubId {
        self.value.id()
    }

    pub(super) fn initial_events(self) -> EntityEvents<XPubEvent> {
        let xpub = self.value.inner;
        EntityEvents::init([
            XPubEvent::XpubInitialized {
                db_uuid: self.db_uuid,
                account_id: self.account_id,
                fingerprint: xpub.fingerprint(),
                parent_fingerprint: xpub.parent_fingerprint,
                xpub,
                original: self.original,
                derivation_path: self.value.derivation,
            },
            XPubEvent::XpubNameUpdated {
                name: self.key_name,
            },
        ])
    }
}

impl TryFrom<EntityEvents<XPubEvent>> for AccountXPub {
    type Error = EntityError;
    fn try_from(events: EntityEvents<XPubEvent>) -> Result<Self, Self::Error> {
        let mut builder = AccountXPubBuilder::default();
        for event in events.iter() {
            match event {
                XPubEvent::XpubInitialized {
                    db_uuid,
                    account_id,
                    xpub,
                    derivation_path,
                    ..
                } => {
                    builder = builder
                        .db_uuid(*db_uuid)
                        .account_id(*account_id)
                        .value(XPubValue {
                            inner: *xpub,
                            derivation: derivation_path.as_ref().cloned(),
                        });
                }
                XPubEvent::XpubNameUpdated { name } => {
                    builder = builder.key_name(name.clone());
                }
                _ => (),
            }
        }
        builder.events(events).build()
    }
}