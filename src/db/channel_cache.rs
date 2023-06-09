use thiserror::Error;

use chrono::NaiveDateTime;
use nostr::{secp256k1::XOnlyPublicKey, EventId};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::{
    net::ImageKind,
    types::ChannelMetadata,
    utils::{
        channel_id_from_tags, channel_meta_or_err, event_hash_or_err, millis_to_naive_or_err,
        ns_event_to_millis, public_key_or_err,
    },
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Error parsing JSON content into nostr::Metadata: {0}")]
    JsonToMetadata(String),

    #[error("Can't update channel without id")]
    ChannelNotInDatabase,

    #[error("Not found channel to update: channel_id: {0}")]
    NotFoundChannelToUpdate(nostr::EventId),

    #[error("Not found channel id inside event tags: event_hash: {0}")]
    NotFoundChannelInTags(nostr::EventId),

    #[error("Event need to be confirmed")]
    NotConfirmedEvent(nostr::EventId),

    #[error("{0}")]
    FromImageCache(#[from] crate::db::image_cache::Error),
}

use super::{DbEvent, ImageDownloaded};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCache {
    pub channel_id: nostr::EventId,
    pub creator_pubkey: XOnlyPublicKey,
    pub created_at: NaiveDateTime,
    pub updated_event_hash: Option<nostr::EventId>,
    pub updated_at: Option<NaiveDateTime>,
    pub metadata: ChannelMetadata,
    pub image_cache: Option<ImageDownloaded>,
    pub members: Vec<XOnlyPublicKey>,
}
impl ChannelCache {
    pub async fn insert_member_from_event(
        cache_pool: &SqlitePool,
        db_event: &DbEvent,
    ) -> Result<u64, Error> {
        let channel_id = channel_id_from_tags(&db_event.tags)
            .ok_or(Error::NotFoundChannelInTags(db_event.event_hash.to_owned()))?;
        Self::insert_member(cache_pool, &channel_id, &db_event.pubkey).await
    }

    pub async fn insert_member(
        cache_pool: &SqlitePool,
        channel_id: &nostr::EventId,
        member: &XOnlyPublicKey,
    ) -> Result<u64, Error> {
        let query =
            "INSERT OR IGNORE INTO channel_member_map (channel_id, public_key) VALUES (?, ?)";

        let output = sqlx::query(query)
            .bind(channel_id.to_string())
            .bind(member.to_string())
            .execute(cache_pool)
            .await?;

        Ok(output.rows_affected())
    }

    pub async fn fetch_by_creator(
        cache_pool: &SqlitePool,
        creator_pubkey: &XOnlyPublicKey,
    ) -> Result<Vec<ChannelCache>, Error> {
        let query = "SELECT * FROM channel_cache WHERE creator_pubkey = ?;";
        let mut results = sqlx::query_as::<_, ChannelCache>(query)
            .bind(creator_pubkey.to_string())
            .fetch_all(cache_pool)
            .await?;

        for channel_cache in &mut results {
            channel_cache.fetch_img_cache(cache_pool).await?;
            channel_cache.fetch_members(cache_pool).await?;
        }

        Ok(results)
    }

    pub async fn fetch_by_channel_id(
        cache_pool: &SqlitePool,
        channel_id: &nostr::EventId,
    ) -> Result<Option<ChannelCache>, Error> {
        let query = "SELECT * FROM channel_cache WHERE creation_event_hash = ?;";
        let mut result = sqlx::query_as::<_, ChannelCache>(query)
            .bind(channel_id.to_string())
            .fetch_optional(cache_pool)
            .await?;
        if let Some(cache) = &mut result {
            cache.fetch_img_cache(cache_pool).await?;
            cache.fetch_members(cache_pool).await?;
        }
        Ok(result)
    }

    // If the channel is not in the database, it will be inserted.
    pub async fn fetch_insert(
        cache_pool: &SqlitePool,
        ns_event: &nostr::Event,
    ) -> Result<ChannelCache, Error> {
        let metadata = nostr::Metadata::from_json(&ns_event.content)
            .map_err(|_| Error::JsonToMetadata(ns_event.content.clone()))?;
        let channel_id = &ns_event.id;
        let creator_pubkey = &ns_event.pubkey;
        let created_at_millis = ns_event_to_millis(ns_event.created_at);

        if let Some(channel_cache) = Self::fetch_by_channel_id(cache_pool, channel_id).await? {
            return Ok(channel_cache);
        }

        let insert_query = r#"
            INSERT INTO channel_cache
                (creation_event_hash, creator_pubkey, created_at, metadata)
            VALUES (?1, ?2, ?3, ?4)
        "#;
        sqlx::query(insert_query)
            .bind(channel_id.to_string())
            .bind(creator_pubkey.to_string())
            .bind(created_at_millis)
            .bind(metadata.as_json())
            .execute(cache_pool)
            .await?;

        let channel_cache = Self::fetch_by_channel_id(cache_pool, channel_id)
            .await?
            .ok_or(Error::ChannelNotInDatabase)?;

        Ok(channel_cache)
    }

    pub async fn update(
        cache_pool: &SqlitePool,
        ns_event: &nostr::Event,
    ) -> Result<ChannelCache, Error> {
        // Only updates for kind 41 coming from the same channel_id.
        // the channel id is inside the E tag
        // It's possible to receive a kind 41 before a kind 40.
        let channel_id = channel_id_from_tags(&ns_event.tags)
            .ok_or(Error::NotFoundChannelInTags(ns_event.id.to_owned()))?;

        // Check if channel already exists in the database or error
        Self::fetch_by_channel_id(cache_pool, &channel_id)
            .await?
            .ok_or(Error::NotFoundChannelToUpdate(channel_id.to_owned()))?;

        let metadata = nostr::Metadata::from_json(&ns_event.content)
            .map_err(|_| Error::JsonToMetadata(ns_event.content.clone()))?;
        let updated_event_hash = ns_event.id;
        let updated_at_millis = ns_event_to_millis(ns_event.created_at);

        let update_query = r#"
            UPDATE channel_cache
            SET metadata=?, updated_event_hash=?, updated_at=?
            WHERE creation_event_hash = ?
        "#;

        sqlx::query(update_query)
            .bind(metadata.as_json())
            .bind(updated_event_hash.to_string())
            .bind(updated_at_millis)
            .bind(channel_id.to_string())
            .execute(cache_pool)
            .await?;

        let channel_cache = Self::fetch_by_channel_id(cache_pool, &channel_id)
            .await?
            .ok_or(Error::ChannelNotInDatabase)?;

        Ok(channel_cache)
    }

    async fn fetch_img_cache(
        &mut self,
        cache_pool: &sqlx::Pool<sqlx::Sqlite>,
    ) -> Result<(), Error> {
        if self.metadata.picture.is_some() {
            let event_hash = self.last_event_hash();
            self.image_cache =
                ImageDownloaded::fetch(cache_pool, event_hash, ImageKind::Channel).await?;
        }
        Ok(())
    }

    async fn fetch_members(&mut self, cache_pool: &SqlitePool) -> Result<(), Error> {
        self.members = fetch_channel_members(cache_pool, &self.channel_id).await?;
        Ok(())
    }

    pub fn last_event_hash(&self) -> &EventId {
        self.updated_event_hash.as_ref().unwrap_or(&self.channel_id)
    }
}

async fn fetch_channel_members(
    cache_pool: &SqlitePool,
    channel_id: &EventId,
) -> Result<Vec<XOnlyPublicKey>, Error> {
    let query = "SELECT public_key FROM channel_member_map WHERE channel_id = ?;";
    let rows = sqlx::query(query)
        .bind(channel_id.to_string())
        .fetch_all(cache_pool)
        .await?;

    let mut members = Vec::new();
    for row in rows {
        let member = row.try_get::<String, &str>("public_key")?;
        let member = public_key_or_err(&member, "public_key")?;
        members.push(member);
    }

    Ok(members)
}

impl sqlx::FromRow<'_, SqliteRow> for ChannelCache {
    fn from_row(row: &'_ SqliteRow) -> Result<Self, sqlx::Error> {
        let metadata: String = row.try_get("metadata")?;
        let metadata = channel_meta_or_err(&metadata, "metadata")?;

        let channel_id: String = row.try_get("creation_event_hash")?;
        let channel_id = event_hash_or_err(&channel_id, "creation_event_hash")?;

        let updated_event_hash: Option<String> = row.get("updated_event_hash");
        let updated_event_hash = updated_event_hash
            .map(|h| event_hash_or_err(&h, "updated_event_hash"))
            .transpose()?;

        let creator_pubkey = row.try_get::<String, &str>("creator_pubkey")?;
        let creator_pubkey = public_key_or_err(&creator_pubkey, "creator_pubkey")?;

        let created_at: i64 = row.try_get("created_at")?;
        let created_at = millis_to_naive_or_err(created_at, "created_at")?;

        let updated_at: Option<i64> = row.get("updated_at");
        let updated_at = updated_at
            .map(|date| millis_to_naive_or_err(date, "updated_at"))
            .transpose()?;

        Ok(Self {
            metadata,
            created_at,
            updated_at,
            channel_id,
            creator_pubkey,
            updated_event_hash,
            image_cache: None,
            members: vec![],
        })
    }
}
