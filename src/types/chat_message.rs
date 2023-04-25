use chrono::NaiveDateTime;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::{
    db::{DbContact, DbEvent, DbMessage},
    error::Error,
};

pub trait EventLike {
    fn created_at(&self) -> i64;
    fn pubkey(&self) -> XOnlyPublicKey;
}

impl EventLike for nostr_sdk::Event {
    fn created_at(&self) -> i64 {
        self.created_at.as_i64()
    }
    fn pubkey(&self) -> XOnlyPublicKey {
        self.pubkey.clone()
    }
}

impl EventLike for DbEvent {
    fn created_at(&self) -> i64 {
        self.created_at.timestamp_millis()
    }
    fn pubkey(&self) -> XOnlyPublicKey {
        self.pubkey.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub msg_id: i64,
    /// Message created at using unix timestamp
    pub created_at: NaiveDateTime,
    /// Decrypted message content
    pub content: String,
    /// Pub key of the author of the message
    pub from_pubkey: XOnlyPublicKey,
    pub is_from_user: bool,
    pub petname: Option<String>,
}

impl ChatMessage {
    pub fn from_db_message(
        db_message: &DbMessage,
        is_from_user: bool,
        contact: &DbContact,
        content: &str,
    ) -> Result<Self, Error> {
        let msg_id = db_message
            .msg_id()
            .ok_or(Error::MissingMessageIdForContactUpdate)?;
        Ok(Self {
            msg_id,
            content: content.to_owned(),
            created_at: db_message.created_at(),
            from_pubkey: db_message.from_pubkey(),
            is_from_user,
            petname: contact.get_petname(),
        })
    }
}
