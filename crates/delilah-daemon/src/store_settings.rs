//! Non-secret settings, one JSON value per key.

use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::Row;

use crate::store::{Store, StoreError};

impl Store {
    pub async fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, StoreError> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(serde_json::from_str(
                &row.try_get::<String, _>("value")?,
            )?)),
            None => Ok(None),
        }
    }

    pub async fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
            .bind(key)
            .bind(serde_json::to_string(value)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Bumps the counter in setting `key` and returns it, atomically: at least
    /// `floor + 1`, and never the same number twice.
    pub async fn next_counter(&self, key: &str, floor: u32) -> Result<u32, StoreError> {
        let value: String = sqlx::query_scalar(
            "INSERT INTO settings (key, value) VALUES (?1, CAST(?2 + 1 AS TEXT))
             ON CONFLICT (key) DO UPDATE SET value = CAST(MAX(CAST(value AS INTEGER), ?2) + 1 AS TEXT)
             RETURNING value",
        )
        .bind(key)
        .bind(floor)
        .fetch_one(&self.pool)
        .await?;
        Ok(serde_json::from_str(&value)?)
    }
}
