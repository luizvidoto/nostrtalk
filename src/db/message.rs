use std::str::FromStr;

use crate::{
    error::{Error, FromDbEventError},
    utils::{handle_decode_error, millis_to_naive_or_err, pubkey_or_err},
};
use chrono::NaiveDateTime;
use nostr_sdk::{nips::nip04, prelude::UncheckedUrl, secp256k1::XOnlyPublicKey, Keys, Url};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::DbEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbMessage {
    msg_id: Option<i64>,
    encrypted_content: String,
    from_pubkey: XOnlyPublicKey,
    to_pubkey: XOnlyPublicKey,
    event_id: Option<i64>,
    created_at: chrono::NaiveDateTime,
    updated_at: chrono::NaiveDateTime,
    status: MessageStatus,
    relay_url: Option<UncheckedUrl>,
}

impl DbMessage {
    const FETCH_QUERY: &'static str =
        "SELECT msg_id, content, from_pubkey, to_pubkey, event_id, created_at, updated_at, status, relay_url FROM message";

    pub fn is_local(&self, own_pubkey: &XOnlyPublicKey) -> bool {
        own_pubkey == &self.from_pubkey
    }
    pub fn is_unseen(&self) -> bool {
        self.status.is_unseen()
    }
    pub fn to_pubkey(&self) -> XOnlyPublicKey {
        self.to_pubkey.to_owned()
    }
    pub fn from_pubkey(&self) -> XOnlyPublicKey {
        self.from_pubkey.to_owned()
    }

    pub fn msg_id(&self) -> Option<i64> {
        self.msg_id
    }
    pub fn created_at(&self) -> NaiveDateTime {
        self.created_at.to_owned()
    }
    pub fn update_status(&mut self, status: MessageStatus) {
        self.status = status;
    }
    pub fn with_id(mut self, id: i64) -> Self {
        self.msg_id = Some(id);
        self
    }

    pub fn with_event(mut self, event_id: i64) -> Self {
        self.event_id = Some(event_id);
        self
    }
    pub fn new_local(
        keys: &Keys,
        to_pubkey: &XOnlyPublicKey,
        message: &str,
    ) -> Result<Self, Error> {
        let secret_key = keys.secret_key()?;
        let encrypted_content = nip04::encrypt(&secret_key, &to_pubkey, message)
            .map_err(|e| Error::EncryptionError(e.to_string()))?;
        Ok(Self {
            msg_id: None,
            encrypted_content,
            from_pubkey: keys.public_key(),
            to_pubkey: to_pubkey.to_owned(),
            event_id: None,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            status: MessageStatus::Offline,
            relay_url: None,
        })
    }
    pub fn from_db_event(db_event: DbEvent, relay_url: Option<Url>) -> Result<Self, Error> {
        let (to_pub, event_id) = Self::extract_to_pub_and_event_id(&db_event)?;
        Ok(Self {
            msg_id: None,
            encrypted_content: db_event.content.to_owned(),
            from_pubkey: db_event.pubkey.clone(),
            to_pubkey: to_pub,
            event_id: Some(event_id),
            created_at: db_event.created_at,
            updated_at: db_event.created_at,
            status: MessageStatus::Delivered,
            relay_url: relay_url.map(|url| url.into()),
        })
    }
    pub fn decrypt_message(&self, keys: &Keys) -> Result<String, Error> {
        let secret_key = keys.secret_key()?;
        if self.is_local(&keys.public_key()) {
            nip04::decrypt(&secret_key, &self.to_pubkey, &self.encrypted_content)
                .map_err(|e| Error::DecryptionError(e.to_string()))
        } else {
            nip04::decrypt(&secret_key, &self.from_pubkey, &self.encrypted_content)
                .map_err(|e| Error::DecryptionError(e.to_string()))
        }
    }

    fn extract_to_pub_and_event_id(
        db_event: &DbEvent,
    ) -> Result<(XOnlyPublicKey, i64), FromDbEventError> {
        let tag = db_event.tags.get(0).ok_or(FromDbEventError::NoTags)?;
        match tag {
            nostr_sdk::Tag::PubKey(to_pub, _url) => {
                let event_id = db_event.event_id.ok_or_else(|| {
                    FromDbEventError::EventNotInDatabase(db_event.event_hash.clone())
                })?;
                Ok((to_pub.clone(), event_id))
            }
            _ => Err(FromDbEventError::WrongTag),
        }
    }

    pub async fn fetch(pool: &SqlitePool, criteria: Option<&str>) -> Result<Vec<DbMessage>, Error> {
        let sql = Self::FETCH_QUERY.to_owned();
        let sql = match criteria {
            None => sql,
            Some(crit) => format!("{} WHERE {}", sql, crit),
        };
        let messages = sqlx::query_as::<_, DbMessage>(&sql).fetch_all(pool).await?;

        Ok(messages)
    }

    pub async fn insert_message(pool: &SqlitePool, db_message: &DbMessage) -> Result<i64, Error> {
        let sql = r#"
            INSERT OR IGNORE INTO message (content, from_pubkey, to_pubkey, event_id, created_at, updated_at, status, relay_url)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#;

        let output = sqlx::query(sql)
            .bind(&db_message.encrypted_content)
            .bind(&db_message.from_pubkey.to_string())
            .bind(&db_message.to_pubkey.to_string())
            .bind(&db_message.event_id)
            .bind(&db_message.created_at.timestamp_millis())
            .bind(&db_message.updated_at.timestamp_millis())
            .bind(&db_message.status.to_i32())
            .bind(&db_message.relay_url.as_ref().map(|url| url.to_string()))
            .execute(pool)
            .await?;

        Ok(output.last_insert_rowid())
    }

    pub async fn update_message_status(
        pool: &SqlitePool,
        db_message: &DbMessage,
    ) -> Result<(), Error> {
        if let Some(msg_id) = db_message.msg_id {
            let sql = r#"
            UPDATE message 
            SET status = ?1
            WHERE msg_id = ?2
            "#;

            sqlx::query(sql)
                .bind(&db_message.status.to_i32())
                .bind(&msg_id)
                .execute(pool)
                .await?;

            Ok(())
        } else {
            Err(Error::MessageNotInDatabase)
        }
    }
}

impl sqlx::FromRow<'_, SqliteRow> for DbMessage {
    fn from_row(row: &'_ SqliteRow) -> Result<Self, sqlx::Error> {
        let created_at =
            millis_to_naive_or_err(row.try_get::<i64, &str>("created_at")?, "created_at")?;
        let updated_at =
            millis_to_naive_or_err(row.try_get::<i64, &str>("updated_at")?, "updated_at")?;
        let from_pubkey =
            pubkey_or_err(&row.try_get::<String, &str>("from_pubkey")?, "from_pubkey")?;
        let to_pubkey = pubkey_or_err(&row.try_get::<String, &str>("to_pubkey")?, "to_pubkey")?;

        Ok(DbMessage {
            msg_id: row.get::<Option<i64>, &str>("msg_id"),
            event_id: row.get::<Option<i64>, &str>("event_id"),
            encrypted_content: row.try_get::<String, &str>("content")?,
            from_pubkey,
            to_pubkey,
            created_at,
            updated_at,
            status: MessageStatus::from_i32(row.try_get::<i32, &str>("status")?),
            relay_url: row
                .get::<Option<String>, &str>("relay_url")
                .map(|s| {
                    UncheckedUrl::from_str(&s).map_err(|e| handle_decode_error(e, "relay_url"))
                })
                .transpose()?,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MessageStatus {
    Offline = 0,
    Delivered = 1,
    Seen = 2,
}

impl MessageStatus {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => MessageStatus::Offline,
            1 => MessageStatus::Delivered,
            _ => MessageStatus::Seen,
        }
    }

    pub fn to_i32(self) -> i32 {
        self as i32
    }
    pub fn is_unseen(&self) -> bool {
        match self {
            MessageStatus::Offline => true,
            MessageStatus::Delivered => true,
            MessageStatus::Seen => false,
        }
    }
}

pub struct DbChat<'a> {
    pub from_pubkey: &'a XOnlyPublicKey,
    pub to_pubkey: &'a XOnlyPublicKey,
}

impl<'a> DbChat<'a> {
    pub fn new(from_pubkey: &'a XOnlyPublicKey, to_pubkey: &'a XOnlyPublicKey) -> Self {
        Self {
            from_pubkey,
            to_pubkey,
        }
    }

    pub async fn fetch_unseen_count(&self, pool: &SqlitePool) -> Result<u8, Error> {
        let sql = r#"
            SELECT COUNT(*)
            FROM message
            WHERE 
                (
                    (from_pubkey = ?1 AND to_pubkey = ?2) OR 
                    (from_pubkey = ?2 AND to_pubkey = ?1)
                ) AND 
                (status = ?3 OR status = ?4)
            "#;
        let count: i64 = sqlx::query_scalar(sql)
            .bind(self.from_pubkey.to_string())
            .bind(self.to_pubkey.to_string())
            .bind(MessageStatus::Offline.to_i32())
            .bind(MessageStatus::Delivered.to_i32())
            .fetch_one(pool)
            .await?;

        Ok(count as u8)
    }

    pub async fn fetch_chat(&self, pool: &SqlitePool) -> Result<Vec<DbMessage>, Error> {
        let sql = r#"
            SELECT *
            FROM message
            WHERE (from_pubkey = ?1 AND to_pubkey = ?2) OR (from_pubkey = ?2 AND to_pubkey = ?1)
            ORDER BY created_at
        "#;

        let messages = sqlx::query_as::<_, DbMessage>(sql)
            .bind(self.from_pubkey.to_string())
            .bind(self.to_pubkey.to_string())
            .fetch_all(pool)
            .await?;

        Ok(messages)
    }

    pub async fn fetch_last_msg_chat(&self, pool: &SqlitePool) -> Result<Option<DbMessage>, Error> {
        let sql = r#"
            SELECT *
            FROM message
            WHERE (from_pubkey = ?1 AND to_pubkey = ?2) OR (from_pubkey = ?2 AND to_pubkey = ?1)
            ORDER BY created_at DESC
            LIMIT 1
        "#;

        let last_message = sqlx::query_as::<_, DbMessage>(sql)
            .bind(self.from_pubkey.to_string())
            .bind(self.to_pubkey.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(last_message)
    }
}
