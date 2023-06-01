use nostr::{secp256k1::XOnlyPublicKey, Filter, Kind, Timestamp};

use crate::db::DbContact;

pub fn contact_list_metadata(contact_list: &[DbContact]) -> Vec<Filter> {
    contact_list
        .iter()
        .map(|c| {
            Filter::new()
                .author(c.pubkey().to_string())
                .kind(Kind::Metadata)
        })
        .collect::<Vec<_>>()
}
pub fn user_metadata(pubkey: XOnlyPublicKey) -> Filter {
    Filter::new()
        .author(pubkey.to_string())
        .kind(Kind::Metadata)
}

pub fn contact_list_filter(public_key: XOnlyPublicKey, last_timestamp_secs: u64) -> Filter {
    let user_contact_list = Filter::new()
        .author(public_key.to_string())
        .kind(Kind::ContactList)
        .since(Timestamp::from(last_timestamp_secs));
    user_contact_list
}

pub fn messages_filter(public_key: XOnlyPublicKey, last_timestamp_secs: u64) -> Vec<Filter> {
    let sent_msgs = Filter::new()
        .kind(nostr::Kind::EncryptedDirectMessage)
        .author(public_key.to_string())
        .since(Timestamp::from(last_timestamp_secs));
    let recv_msgs = Filter::new()
        .kind(nostr::Kind::EncryptedDirectMessage)
        .pubkey(public_key)
        .since(Timestamp::from(last_timestamp_secs));
    vec![sent_msgs, recv_msgs]
}

pub fn channel_search_filter(channel_id: &str) -> Filter {
    // .search(search_term)
    // .hashtag(search_term)
    let mut channel_filter = Filter::new()
        .kind(Kind::ChannelCreation)
        .limit(CHANNEL_SEARCH_LIMIT);

    if !channel_id.is_empty() {
        channel_filter = channel_filter.id(channel_id);
    }

    channel_filter
}

pub fn channel_details_filter(channel_id: nostr::EventId) -> Vec<Filter> {
    vec![
        Filter::new()
            .kind(Kind::ChannelMetadata)
            .event(channel_id)
            .limit(10),
        Filter::new()
            .kind(Kind::ChannelMessage)
            .event(channel_id)
            .limit(CHANNEL_DETAILS_LIMIT),
    ]
}
const CHANNEL_SEARCH_LIMIT: usize = 10;
const CHANNEL_DETAILS_LIMIT: usize = 1000;
