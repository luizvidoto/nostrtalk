use futures::channel::mpsc;
use nostr_sdk::{Keys, Metadata, RelayMessage, Url};
use sqlx::SqlitePool;

use crate::{
    db::{
        update_user_meta, DbChat, DbContact, DbEvent, DbMessage, DbRelay, DbRelayResponse,
        MessageStatus,
    },
    error::Error,
    net::{events::nostr::NostrInput, process_async_fn, process_async_with_event},
    types::ChatMessage,
};

use super::{frontend::SpecificEvent, Event};

#[derive(Debug, Clone)]
pub enum BackEndInput {
    Ok(Event),
    ToggleRelayRead((DbRelay, bool)),
    ToggleRelayWrite((DbRelay, bool)),
    AddRelayToDb(DbRelay),
    DeleteRelayFromDb(DbRelay),
    StorePendingEvent(nostr_sdk::Event),
    StoreEvent((nostr_sdk::Url, nostr_sdk::Event)),
    StoreRelayMessage((nostr_sdk::Url, nostr_sdk::RelayMessage)),
    LatestVersion(String),
    Shutdown,
    None,
}

pub async fn backend_processing(
    pool: &SqlitePool,
    keys: &Keys,
    input: BackEndInput,
    _sender: &mut mpsc::Sender<BackEndInput>,
) -> Event {
    match input {
        BackEndInput::Ok(event) => event,
        BackEndInput::None => Event::None,
        // --- REQWEST ---
        BackEndInput::LatestVersion(version) => Event::LatestVersion(version),
        BackEndInput::Shutdown => Event::None,
        // --- TO DATABASE ---
        BackEndInput::DeleteRelayFromDb(db_relay) => {
            process_async_with_event(db_delete_relay(&pool, db_relay)).await
        }
        BackEndInput::ToggleRelayRead((mut db_relay, read)) => {
            db_relay.read = read;
            match DbRelay::update(&pool, &db_relay).await {
                Ok(_) => Event::RelayUpdated(db_relay.clone()),
                Err(e) => Event::Error(e.to_string()),
            }
        }
        BackEndInput::ToggleRelayWrite((mut db_relay, write)) => {
            db_relay.write = write;
            match DbRelay::update(&pool, &db_relay).await {
                Ok(_) => Event::RelayUpdated(db_relay.clone()),
                Err(e) => Event::Error(e.to_string()),
            }
        }
        BackEndInput::AddRelayToDb(db_relay) => {
            process_async_with_event(db_add_relay(&pool, db_relay)).await
        }
        BackEndInput::StorePendingEvent(nostr_event) => {
            process_async_fn(insert_pending_event(&pool, &keys, nostr_event), |event| {
                event
            })
            .await
        }
        BackEndInput::StoreEvent((relay_url, nostr_event)) => {
            process_async_with_event(insert_event(&pool, &keys, nostr_event, &relay_url)).await
        }

        BackEndInput::StoreRelayMessage((relay_url, relay_message)) => {
            process_async_fn(
                on_relay_message(&pool, &relay_url, &relay_message),
                |event| event,
            )
            .await
        }
    }
}

pub async fn insert_specific_kind(
    pool: &SqlitePool,
    keys: &Keys,
    relay_url: Option<&Url>,
    db_event: &DbEvent,
) -> Result<Option<SpecificEvent>, Error> {
    let event = match db_event.kind {
        nostr_sdk::Kind::Metadata => {
            tracing::info!(
                "Received metadata event for public key: {}",
                db_event.pubkey
            );
            tracing::info!("{:?}", db_event);
            let last_update = db_event.created_at_from_relay();
            let metadata = Metadata::from_json(&db_event.content)
                .map_err(|_| Error::JsonToMetadata(db_event.content.to_string()))?;

            if db_event.pubkey == keys.public_key() {
                update_user_meta(pool, &metadata, last_update).await?;
                Some(SpecificEvent::UpdatedUserProfileMeta(metadata))
            } else {
                if let Some(mut db_contact) = DbContact::fetch_one(pool, &db_event.pubkey).await? {
                    db_contact = db_contact.with_profile_meta(&metadata, last_update);
                    DbContact::update(&pool, &db_contact).await?;
                    tracing::info!("Updated contact with profile metadata: {:?}", db_contact);
                    Some(SpecificEvent::UpdatedContactMetadata(db_contact))
                } else {
                    None
                }
            }
        }
        nostr_sdk::Kind::EncryptedDirectMessage => {
            // Convert DbEvent to DbMessage
            let db_message = DbMessage::from_db_event(db_event, relay_url)?;
            let event = received_dm(pool, keys, db_message).await?;
            Some(event)
        }
        nostr_sdk::Kind::RecommendRelay => {
            println!("--- RecommendRelay ---");
            dbg!(&db_event);
            None
        }
        nostr_sdk::Kind::ContactList => {
            if db_event.pubkey == keys.public_key() {
                println!("--- My ContactList ---");
                let db_contacts: Vec<_> = db_event
                    .tags
                    .iter()
                    .filter_map(|t| DbContact::from_tag(t).ok())
                    .collect();
                for db_contact in &db_contacts {
                    if let Err(e) = insert_contact(keys, pool, db_contact).await {
                        tracing::error!("{}", e);
                    }
                }
                Some(SpecificEvent::RelayContactsImported(db_contacts))
            } else {
                println!("*** Others ContactList That Im in ***");
                None
            }
        }
        nostr_sdk::Kind::ChannelCreation => {
            // println!("--- ChannelCreation ---");
            // dbg!(db_event);
            None
        }
        nostr_sdk::Kind::ChannelMetadata => {
            // println!("--- ChannelMetadata ---");
            // dbg!(db_event);
            None
        }
        nostr_sdk::Kind::ChannelMessage => {
            // println!("--- ChannelMessage ---");
            // dbg!(db_event);
            None
        }
        nostr_sdk::Kind::ChannelHideMessage => {
            // println!("--- ChannelHideMessage ---");
            // dbg!(db_event);
            None
        }
        nostr_sdk::Kind::ChannelMuteUser => {
            // println!("--- ChannelMuteUser ---");
            // dbg!(db_event);
            None
        }
        // Kind::EventDeletion => todo!(),
        // Kind::PublicChatReserved45 => todo!(),
        // Kind::PublicChatReserved46 => todo!(),
        // Kind::PublicChatReserved47 => todo!(),
        // Kind::PublicChatReserved48 => todo!(),
        // Kind::PublicChatReserved49 => todo!(),
        // Kind::ZapRequest => todo!(),
        // Kind::Zap => todo!(),
        // Kind::MuteList => todo!(),
        // Kind::PinList => todo!(),
        // Kind::RelayList => todo!(),
        // Kind::Authentication => todo!(),
        _other => None,
    };

    Ok(event)
}

pub async fn handle_insert_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
    relay_url: Option<&Url>,
    is_pending: bool,
) -> Result<Event, Error> {
    tracing::info!(
        "Inserting [{}] event",
        if is_pending { "pending" } else { "confirmed" },
    );
    tracing::debug!("{:?}", event);

    let mut db_event = if is_pending {
        DbEvent::pending_event(event)?
    } else {
        DbEvent::confirmed_event(event)?
    };
    if let Some(url) = relay_url {
        db_event = db_event.with_relay(url);
    }

    let (row_id, rows_changed) = DbEvent::insert(pool, &db_event).await?;
    db_event = db_event.with_id(row_id);

    // after db_event is inserted, we can update the relay_response
    if let Some(url) = relay_url {
        let _ev = relay_response_ok(pool, &db_event, url).await?;
    }

    if rows_changed == 0 {
        return Ok(Event::None);
    }

    let specific_event = insert_specific_kind(pool, keys, relay_url, &db_event).await?;
    if is_pending {
        Ok(Event::LocalPendingEvent((db_event, specific_event)))
    } else {
        Ok(Event::EventInserted((db_event, specific_event)))
    }
}

pub async fn insert_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
    relay_url: &Url,
) -> Result<Event, Error> {
    handle_insert_event(pool, keys, event, Some(relay_url), false).await
}

pub async fn insert_pending_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
) -> Result<Event, Error> {
    handle_insert_event(pool, keys, event, None, true).await
}

pub async fn received_dm(
    pool: &SqlitePool,
    keys: &Keys,
    db_message: DbMessage,
) -> Result<SpecificEvent, Error> {
    tracing::debug!("Inserting external message");
    // Insert message into the database and get the message ID
    let msg_id = DbMessage::insert_message(pool, &db_message).await?;
    let db_message = db_message.with_id(msg_id);

    // Decrypt the message content
    let content = db_message.decrypt_message(keys)?;

    // Determine if the message is from the user or received from another user
    let (contact_pubkey, is_from_user) = if db_message.im_author(&keys.public_key()) {
        (db_message.to_pubkey(), true)
    } else {
        (db_message.from_pubkey(), false)
    };

    // Fetch the associated contact from the database
    match DbContact::fetch_one(pool, &contact_pubkey).await? {
        Some(mut db_contact) => {
            // Update last message and contact in the database
            let chat_message =
                ChatMessage::from_db_message(&db_message, is_from_user, &db_contact, &content)?;
            db_contact = DbContact::new_message(pool, &db_contact, &chat_message).await?;
            Ok(SpecificEvent::ReceivedDM((db_contact, chat_message)))
        }
        None => {
            // Create a new contact and insert it into the database
            let mut db_contact = DbContact::new(&contact_pubkey);
            insert_contact(keys, pool, &db_contact).await?;

            // Update last message and contact in the database
            let chat_message =
                ChatMessage::from_db_message(&db_message, is_from_user, &db_contact, &content)?;
            db_contact = DbContact::new_message(pool, &db_contact, &chat_message).await?;

            Ok(SpecificEvent::NewDMAndContact((db_contact, chat_message)))
        }
    }
}

pub async fn relay_response_ok(
    pool: &SqlitePool,
    db_event: &DbEvent,
    relay_url: &Url,
) -> Result<Event, Error> {
    let mut relay_response = DbRelayResponse::from_response(
        true,
        db_event.event_id()?,
        &db_event.event_hash,
        relay_url,
        "",
    );
    let id = DbRelayResponse::insert(pool, &relay_response).await?;
    relay_response = relay_response.with_id(id);
    Ok(Event::UpdateWithRelayResponse {
        relay_response,
        db_event: db_event.clone(),
        db_message: None,
    })
}

pub async fn insert_contact(
    keys: &Keys,
    pool: &SqlitePool,
    db_contact: &DbContact,
) -> Result<Event, Error> {
    if &keys.public_key() == db_contact.pubkey() {
        return Err(Error::SameContactInsert);
    }
    DbContact::insert(pool, &db_contact).await?;
    Ok(Event::ContactCreated(db_contact.clone()))
}

pub async fn update_contact(
    keys: &Keys,
    pool: &SqlitePool,
    db_contact: &DbContact,
) -> Result<Event, Error> {
    if &keys.public_key() == db_contact.pubkey() {
        return Err(Error::SameContactUpdate);
    }
    DbContact::update(pool, &db_contact).await?;
    Ok(Event::ContactUpdated(db_contact.clone()))
}

pub async fn delete_contact(pool: &SqlitePool, db_contact: &DbContact) -> Result<Event, Error> {
    DbContact::delete(pool, &db_contact).await?;
    Ok(Event::ContactDeleted(db_contact.clone()))
}

// pub async fn import_contacts(
//     keys: &Keys,
//     pool: &SqlitePool,
//     db_contacts: &[DbContact],
// ) -> Result<SpecificEvent, Error> {
//     for db_contact in db_contacts {
//         if let Err(e) = insert_contact(keys, pool, db_contact).await {
//             tracing::error!("{}", e);
//         }
//     }
//     Ok(SpecificEvent::ContactsImported(db_contacts.to_vec()))
// }

pub async fn prepare_client(pool: &SqlitePool) -> Result<NostrInput, Error> {
    tracing::info!("Preparing client");
    let relays = DbRelay::fetch(pool, None).await?;
    let last_event = DbEvent::fetch_last(pool).await?;

    Ok(NostrInput::PrepareClient { relays, last_event })
}

pub async fn on_relay_message(
    pool: &SqlitePool,
    relay_url: &Url,
    relay_message: &RelayMessage,
) -> Result<Event, Error> {
    tracing::info!("New relay message: {}", relay_url);
    tracing::debug!("{:?}", relay_message);
    let event = match relay_message {
        RelayMessage::Ok {
            event_id: event_hash,
            status,
            message,
        } => {
            tracing::info!("Relay message: Ok");
            let mut db_event = DbEvent::fetch_one(pool, event_hash)
                .await?
                .ok_or(Error::EventNotInDatabase(event_hash.to_owned()))?;
            let mut db_message = None;

            if !db_event.confirmed {
                db_event = DbEvent::confirm_event(pool, relay_url, db_event).await?;

                if let nostr_sdk::Kind::EncryptedDirectMessage = db_event.kind {
                    db_message = if let Some(db_message) =
                        DbMessage::fetch_one(pool, db_event.event_id()?).await?
                    {
                        let confirmed_db_message =
                            DbMessage::confirm_message(pool, db_message).await?;
                        Some(confirmed_db_message)
                    } else {
                        None
                    };
                }
            }

            let mut relay_response = DbRelayResponse::from_response(
                *status,
                db_event.event_id()?,
                event_hash,
                relay_url,
                message,
            );
            let id = DbRelayResponse::insert(pool, &relay_response).await?;
            relay_response = relay_response.with_id(id);
            Event::UpdateWithRelayResponse {
                relay_response,
                db_event,
                db_message,
            }
        }
        RelayMessage::EndOfStoredEvents(_sub_id) => {
            tracing::info!("Relay message: EOE");
            Event::EndOfStoredEvents((relay_url.to_owned(), _sub_id.to_owned()))
        }
        RelayMessage::Event { .. } => {
            tracing::info!("Relay message: Event");
            Event::None
        }
        RelayMessage::Notice { message } => {
            tracing::info!("Relay message: Notice: {}", message);
            Event::None
        }
        RelayMessage::Auth { challenge } => {
            tracing::info!("Relay message: Auth Challenge: {}", challenge);
            Event::None
        }
        RelayMessage::Count {
            subscription_id: _,
            count,
        } => {
            tracing::info!("Relay message: Count: {}", count);
            Event::None
        }
        RelayMessage::Empty => {
            tracing::info!("Relay message: Empty");
            Event::None
        }
    };

    Ok(event)
}

pub async fn add_to_unseen_count(
    pool: &SqlitePool,
    mut db_contact: DbContact,
) -> Result<Event, Error> {
    db_contact = DbContact::add_to_unseen_count(pool, &mut db_contact).await?;
    Ok(Event::ContactUpdated(db_contact))
}

pub async fn fetch_and_decrypt_chat(
    keys: &Keys,
    pool: &SqlitePool,
    mut db_contact: DbContact,
) -> Result<Event, Error> {
    tracing::info!("Fetching chat messages");
    let own_pubkey = keys.public_key();
    let chat = DbChat::new(&own_pubkey, db_contact.pubkey());
    let mut db_messages = chat.fetch_chat(pool).await?;
    let mut chat_messages = vec![];

    tracing::info!("Updating unseen messages to marked as seen");
    for m in db_messages.iter_mut().filter(|m| m.is_unseen()) {
        m.update_status(MessageStatus::Seen);
        DbMessage::update_message_status(pool, m).await?;
    }

    tracing::info!("Decrypting messages");
    for m in &mut db_messages {
        let content = m.decrypt_message(keys)?;
        let is_from_user = m.im_author(&keys.public_key());
        let chat_message = ChatMessage::from_db_message(&m, is_from_user, &db_contact, &content)?;
        chat_messages.push(chat_message);
    }

    db_contact = DbContact::update_unseen_count(pool, &mut db_contact, 0).await?;

    Ok(Event::GotChatMessages((db_contact, chat_messages)))
}

pub async fn fetch_relays_responses(pool: &SqlitePool, event_id: i64) -> Result<Event, Error> {
    let responses = DbRelayResponse::fetch_by_event(pool, event_id).await?;
    Ok(Event::GotRelayResponses(responses))
}

pub async fn db_add_relay(pool: &SqlitePool, db_relay: DbRelay) -> Result<Event, Error> {
    DbRelay::insert(pool, &db_relay).await?;
    Ok(Event::RelayCreated(db_relay))
}

pub async fn fetch_contacts(pool: &SqlitePool) -> Result<Event, Error> {
    let contacts = DbContact::fetch(pool).await?;
    Ok(Event::GotContacts(contacts))
}

pub async fn db_delete_relay(pool: &SqlitePool, db_relay: DbRelay) -> Result<Event, Error> {
    DbRelay::delete(pool, &db_relay).await?;
    Ok(Event::RelayDeleted(db_relay))
}
pub async fn fetch_relays(pool: &SqlitePool) -> Result<Event, Error> {
    let relays = DbRelay::fetch(pool, None).await?;
    Ok(Event::GotRelays(relays))
}
