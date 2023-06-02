#![allow(dead_code)]
use crate::{
    components::chat_contact::ChatContact, db::DbContact, net::ImageKind, types::ChannelMetadata,
};
use chrono::{DateTime, Local, NaiveDateTime, Offset};
use iced::widget::image::Handle;
use image::{ImageBuffer, Luma, Rgba};
use nostr::prelude::*;
use qrcode::QrCode;
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
    str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Nostr Nip 19 Error: {0}")]
    ParseNip19Error(#[from] nostr::nips::nip19::Error),

    #[error("JSON (de)serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid Unix timestamp: {0}")]
    InvalidTimestamp(i64),

    #[error("Invalid Unix timestamp: secs {0}, nanos {1}")]
    InvalidTimestampNanos(i64, u32),

    #[error("{0}")]
    QrError(#[from] qrcode::types::QrError),
}

// Accepts both hex and bech32 keys and returns the hex encoded key
pub fn parse_key(key: String) -> Result<String, Error> {
    // Check if the key is a bech32 encoded key
    let parsed_key = if key.starts_with("npub") {
        XOnlyPublicKey::from_bech32(key)?.to_string()
    } else if key.starts_with("nsec") {
        SecretKey::from_bech32(key)?.display_secret().to_string()
    } else if key.starts_with("note") {
        EventId::from_bech32(key)?.to_hex()
    } else if key.starts_with("nchannel") {
        ChannelId::from_bech32(key)?.to_hex()
    } else {
        // If the key is not bech32 encoded, return it as is
        key
    };
    Ok(parsed_key)
}

pub fn json_reader<P, T: DeserializeOwned>(path: P) -> Result<T, Error>
where
    P: AsRef<Path>,
{
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let buf_reader = BufReader::new(file);
    let content = serde_json::from_reader(buf_reader)?;
    Ok(content)
}

pub fn json_to_string<P>(path: P) -> Result<String, io::Error>
where
    P: AsRef<Path>,
{
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Convert a i64 representing milliseconds since UNIX epoch to an Option<NaiveDateTime>.
///
/// # Arguments
///
/// * `millis` - A i64 representing the number of milliseconds since the UNIX epoch (1970-01-01 00:00:00 UTC)
///
/// # Returns
///
/// * Am Option with a NaiveDateTime representing the given milliseconds since UNIX epoch
pub fn millis_to_naive(millis: i64) -> Result<NaiveDateTime, Error> {
    // Calculate the seconds and nanoseconds components of the input timestamp
    let ts_secs = millis / 1000;
    let ts_ns = (millis % 1000) * 1_000_000;

    // Convert the seconds and nanoseconds to a NaiveDateTime
    NaiveDateTime::from_timestamp_opt(ts_secs, ts_ns as u32).ok_or(Error::InvalidTimestamp(millis))
}

pub fn ns_event_to_millis(created_at: nostr::Timestamp) -> i64 {
    created_at.as_i64() * 1000
}

pub fn ns_event_to_naive(created_at: nostr::Timestamp) -> Result<NaiveDateTime, Error> {
    Ok(millis_to_naive(ns_event_to_millis(created_at))?)
}

pub fn naive_to_event_tt(naive_utc: NaiveDateTime) -> nostr::Timestamp {
    nostr::Timestamp::from(naive_utc.timestamp() as u64)
}

pub fn handle_decode_error<E>(error: E, index: &str) -> sqlx::Error
where
    E: std::error::Error + 'static + Send + Sync,
{
    sqlx::Error::ColumnDecode {
        index: index.into(),
        source: Box::new(error),
    }
}

pub fn millis_to_naive_or_err(millis: i64, index: &str) -> Result<NaiveDateTime, sqlx::Error> {
    Ok(
        millis_to_naive(millis).map_err(|e| sqlx::Error::ColumnDecode {
            index: index.into(),
            source: Box::new(e),
        })?,
    )
}
pub fn public_key_or_err(public_key: &str, index: &str) -> Result<XOnlyPublicKey, sqlx::Error> {
    XOnlyPublicKey::from_str(public_key).map_err(|e| handle_decode_error(e, index))
}
pub fn event_hash_or_err(event_id: &str, index: &str) -> Result<EventId, sqlx::Error> {
    EventId::from_str(event_id).map_err(|e| handle_decode_error(e, index))
}
pub fn url_or_err(url: &str, index: &str) -> Result<Url, sqlx::Error> {
    Url::from_str(url).map_err(|e| handle_decode_error(e, index))
}
pub fn unchecked_url_or_err(url: &str, index: &str) -> Result<UncheckedUrl, sqlx::Error> {
    UncheckedUrl::from_str(url).map_err(|e| handle_decode_error(e, index))
}
pub fn profile_meta_or_err(json: &str, index: &str) -> Result<nostr::Metadata, sqlx::Error> {
    nostr::Metadata::from_json(json).map_err(|e| handle_decode_error(e, index))
}
pub fn channel_meta_or_err(json: &str, index: &str) -> Result<ChannelMetadata, sqlx::Error> {
    ChannelMetadata::from_json(json).map_err(|e| handle_decode_error(e, index))
}
pub fn image_kind_or_err(kind: i32, index: &str) -> Result<ImageKind, sqlx::Error> {
    ImageKind::from_i32(kind).map_err(|e| handle_decode_error(e, index))
}

pub fn chat_matches_search(chat: &ChatContact, search: &str) -> bool {
    let selected_name = chat.contact.select_name();
    selected_name
        .to_lowercase()
        .contains(&search.to_lowercase())
}

pub fn url_matches_search(url: &Url, search: &str) -> bool {
    url.as_str().to_lowercase().contains(&search.to_lowercase())
}

pub fn from_naive_utc_to_local(naive_utc: NaiveDateTime) -> DateTime<Local> {
    DateTime::from_utc(naive_utc, Local::now().offset().fix())
}

pub fn contact_matches_search_full(contact: &DbContact, search: &str) -> bool {
    let pubkey_matches = contact
        .pubkey()
        .to_string()
        .to_lowercase()
        .contains(&search.to_lowercase());
    let petname_matches = contact.get_petname().map_or(false, |petname| {
        petname.to_lowercase().contains(&search.to_lowercase())
    });

    let profile_name_matches = contact.get_profile_name().map_or(false, |name| {
        name.to_lowercase().contains(&search.to_lowercase())
    });

    let display_name_matches = contact.get_display_name().map_or(false, |display_name| {
        display_name.to_lowercase().contains(&search.to_lowercase())
    });

    pubkey_matches || petname_matches || profile_name_matches || display_name_matches
}

pub fn add_ellipsis_trunc(s: &str, max_length: usize) -> String {
    if s.chars().count() > max_length {
        let truncated = s.chars().take(max_length).collect::<String>();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

pub fn qr_code_handle(code: &str) -> Result<Handle, Error> {
    // Encode some data into bits.
    let code = match QrCode::new(code.as_bytes()) {
        Err(e) => {
            tracing::error!("Error creating QR code: {}", e);
            return Err(Error::QrError(e));
        }
        Ok(code) => code,
    };

    // Render the bits into an image.
    let image = code.render::<Luma<u8>>().build();

    // Convert the Luma<u8> image into an Rgba<u8> image
    let rgba_image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
            let pixel = image.get_pixel(x, y);
            let color = pixel[0];
            Rgba([color, color, color, 255]) // Use the grayscale color for each of RGB and set max alpha
        });

    // Get the dimensions
    let (width, height) = rgba_image.dimensions();
    // Get the raw bytes
    let bytes = rgba_image.into_raw();

    Ok(Handle::from_pixels(width, height, bytes)) // Pass the owned bytes
}

/// Hides the middle part of a string with "..."
pub fn hide_string(string: &str, open: usize) -> String {
    let chars: Vec<char> = string.chars().collect();
    let len = chars.len();

    // If open value is greater than half of the string length, return the entire string
    if open >= len / 2 {
        return string.to_string();
    }

    let prefix: String = chars.iter().take(open).collect();
    let suffix: String = chars.iter().rev().take(open).collect();

    format!("{}...{}", prefix, suffix.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hide_string() {
        // the string total chars is 13
        // 0 from each side turns into 0 chars, hide the entire string
        assert_eq!(hide_string("Hello, world!", 0), "...");

        // 2 from each side turns into 4 chars, hide 9 chars
        assert_eq!(hide_string("Hello, world!", 2), "He...d!");

        // 5 from each side turns into 10 chars, hide 3 chars
        assert_eq!(hide_string("Hello, world!", 5), "Hello...orld!");

        // 8 from each side turns into 16 chars, open the entire string
        assert_eq!(hide_string("Hello, world!", 8), "Hello, world!");
    }
}
