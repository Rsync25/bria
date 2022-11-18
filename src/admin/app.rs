use super::error::*;
use super::keys::*;

use crate::account::{keys::*, *};

const BOOTSTRAP_KEY_NAME: &str = "admin_bootstrap_key";

pub struct AdminApp {
    keys: AdminApiKeys,
    accounts: Accounts,
    account_keys: AccountApiKeys,
}

impl AdminApp {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            keys: AdminApiKeys::new(&pool),
            accounts: Accounts::new(&pool),
            account_keys: AccountApiKeys::new(&pool),
        }
    }
}

impl AdminApp {
    pub async fn bootstrap(&self) -> Result<AdminApiKey, AdminApiError> {
        self.keys.create(BOOTSTRAP_KEY_NAME.to_string()).await
    }

    pub async fn authenticate(&self, key: &str) -> Result<(), AdminApiError> {
        self.keys.find_by_key(key).await?;
        Ok(())
    }

    pub async fn account_create(&self, name: String) -> Result<AccountApiKey, AdminApiError> {
        let account = self.accounts.create(name.clone()).await?;
        Ok(self.account_keys.create(name, account.id).await?)
    }
}
