use crate::db::{
    store_last_event_timestamp, Database, DbContact, DbEvent, DbMessage, DbRelayResponse,
};
use crate::error::Error;
use crate::net::contact::{insert_batch_of_contacts, insert_contact};
use crate::net::{
    add_to_unseen_count, fetch_and_decrypt_chat, fetch_relays_responses, Connection,
    APP_TICK_INTERVAL_MILLIS,
};
use crate::types::ChatMessage;
use futures::Future;
use iced::futures::{channel::mpsc, StreamExt};
use iced::{subscription, Subscription};
use nostr_sdk::{Keys, Kind, Url};
use sqlx::SqlitePool;
use std::time::Duration;

use super::BackEndConnection;

pub enum DatabaseState {
    Disconnected {
        keys: Keys,
        in_memory: bool,
        db_conn: BackEndConnection<Message>,
    },
    Connected {
        db_conn: BackEndConnection<Message>,
        receiver: mpsc::UnboundedReceiver<Message>,
        database: Database,
        keys: Keys,
    },
    Processing {
        db_conn: BackEndConnection<Message>,
        receiver: mpsc::UnboundedReceiver<Message>,
        database: Database,
        keys: Keys,
    },
}

pub fn database_connect(keys: &Keys, db_conn: &BackEndConnection<Message>) -> Subscription<Event> {
    struct Backend;
    let id = std::any::TypeId::of::<Backend>();

    subscription::unfold(
        id,
        DatabaseState::Disconnected {
            keys: keys.clone(),
            in_memory: IN_MEMORY,
            db_conn: db_conn.clone(),
        },
        |state| async move {
            match state {
                DatabaseState::Disconnected {
                    keys,
                    in_memory,
                    mut db_conn,
                } => {
                    let (sender, receiver) = mpsc::unbounded();

                    db_conn.with_channel(sender);

                    let database =
                        match Database::new(in_memory, &keys.public_key().to_string()).await {
                            Ok(database) => database,
                            Err(e) => {
                                tracing::error!("Failed to init database");
                                tracing::error!("{}", e);
                                tracing::warn!("Trying again in 2 secs");
                                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                                return (
                                    Event::DbDisconnected,
                                    DatabaseState::Disconnected {
                                        keys,
                                        in_memory,
                                        db_conn,
                                    },
                                );
                            }
                        };

                    (
                        Event::DbConnected,
                        DatabaseState::Processing {
                            database,
                            receiver,
                            keys,
                            db_conn,
                        },
                    )
                }
                DatabaseState::Processing {
                    db_conn,
                    receiver,
                    database,
                    keys,
                } => {
                    db_conn.process_message_queue();
                    // Aguarde o intervalo
                    tokio::time::sleep(Duration::from_millis(APP_TICK_INTERVAL_MILLIS)).await;

                    (
                        Event::DatabaseFinishedProcessing,
                        DatabaseState::Connected {
                            database,
                            receiver,
                            keys,
                            db_conn,
                        },
                    )
                }
                DatabaseState::Connected {
                    database,
                    mut receiver,
                    keys,
                    db_conn,
                } => {
                    let event = futures::select! {
                        message = receiver.select_next_some() => {
                            let event = match message {
                                Message::ProcessDatabaseMessages => {
                                    return (Event::DatabaseProcessing, DatabaseState::Processing {
                                        database,
                                        receiver,
                                        keys,
                                        db_conn,
                                    });
                                }
                                Message::FetchRelayResponses(event_id) => {
                                    process_async_fn(
                                        fetch_relays_responses(&database.pool, event_id),
                                        |responses| Event::GotRelayResponses(responses)
                                    ).await
                                }
                                Message::AddToUnseenCount(db_contact) => {
                                    process_async_fn(
                                        add_to_unseen_count(&database.pool, db_contact),
                                        |contact| Event::ContactUpdated(contact)
                                    ).await
                                }
                                Message::AddContact(db_contact) => {
                                    process_async_fn(
                                        insert_contact(&keys, &database.pool, &db_contact),
                                        |_| Event::ContactCreated(db_contact.clone()),
                                    )
                                    .await
                                }
                                Message::ImportContacts(db_contacts) => {
                                    process_async_fn(
                                        insert_batch_of_contacts(&keys, &database.pool, &db_contacts),
                                        |_| Event::ContactsImported(db_contacts.clone()),
                                    )
                                    .await
                                }
                                Message::UpdateContact(db_contact) => {
                                    process_async_fn(
                                        DbContact::update(&database.pool, &db_contact),
                                        |_| Event::ContactUpdated(db_contact.clone()),
                                    )
                                    .await
                                }
                                Message::DeleteContact(contact) => {
                                    process_async_fn(
                                        DbContact::delete(&database.pool, &contact),
                                        |_| Event::ContactDeleted(contact.clone()),
                                    )
                                    .await
                                }
                                Message::FetchContacts => {
                                    process_async_fn(
                                        DbContact::fetch(&database.pool),
                                        |contacts| Event::GotContacts(contacts),
                                    )
                                    .await
                                }
                                Message::FetchMessages (contact) => {
                                    process_async_fn(
                                        fetch_and_decrypt_chat(&keys, &database.pool, contact),
                                        |(contact, messages)| Event::GotChatMessages((contact, messages)),
                                    )
                                    .await
                                }
                            };

                            event
                        }
                    };
                    (
                        event,
                        DatabaseState::Connected {
                            database,
                            receiver,
                            keys,
                            db_conn,
                        },
                    )
                }
            }
        },
    )
}

async fn process_async_fn<T, E>(
    operation: impl Future<Output = Result<T, E>>,
    success_event_fn: impl Fn(T) -> Event,
) -> Event
where
    E: std::error::Error,
{
    match operation.await {
        Ok(result) => success_event_fn(result),
        Err(e) => Event::Error(e.to_string()),
    }
}

async fn received_encrypted_dm(
    pool: &SqlitePool,
    keys: &Keys,
    db_event: DbEvent,
    relay_url: Option<&Url>,
) -> Result<Event, Error> {
    // Convert DbEvent to DbMessage
    let db_message = DbMessage::from_db_event(db_event, relay_url)?;
    tracing::info!("Inserting external message");

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
            Ok(Event::ReceivedDM((db_contact, chat_message)))
        }
        None => {
            // Create a new contact and insert it into the database
            let mut db_contact = DbContact::new(&contact_pubkey);
            insert_contact(keys, pool, &db_contact).await?;

            // Update last message and contact in the database
            let chat_message =
                ChatMessage::from_db_message(&db_message, is_from_user, &db_contact, &content)?;
            db_contact = DbContact::new_message(pool, &db_contact, &chat_message).await?;

            Ok(Event::NewDMAndContact((db_contact, chat_message)))
        }
    }
}

async fn relay_response_ok(
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

pub async fn received_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
    relay_url: &Url,
) -> Result<Event, Error> {
    let event = insert_event(pool, keys, event, relay_url).await?;

    store_last_event_timestamp(pool).await?;

    Ok(event)
}

async fn insert_specific_kind(
    pool: &SqlitePool,
    keys: &Keys,
    relay_url: Option<&Url>,
    db_event: &DbEvent,
) -> Result<Option<Event>, Error> {
    let event = match db_event.kind {
        Kind::EncryptedDirectMessage => {
            let database_success_event_kind =
                received_encrypted_dm(pool, keys, db_event.clone(), relay_url).await?;
            Some(database_success_event_kind)
        }
        Kind::RecommendRelay => {
            println!("--- RecommendRelay ---");
            dbg!(db_event);
            None
        }
        Kind::ContactList => {
            println!("--- ContactList ---");
            dbg!(db_event);
            None
        }
        Kind::ChannelCreation => {
            // println!("--- ChannelCreation ---");
            // dbg!(db_event);
            None
        }
        Kind::ChannelMetadata => {
            // println!("--- ChannelMetadata ---");
            // dbg!(db_event);
            None
        }
        Kind::ChannelMessage => {
            // println!("--- ChannelMessage ---");
            // dbg!(db_event);
            None
        }
        Kind::ChannelHideMessage => {
            // println!("--- ChannelHideMessage ---");
            // dbg!(db_event);
            None
        }
        Kind::ChannelMuteUser => {
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
        _ => None,
    };

    Ok(event)
}

async fn handle_insert_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
    relay_url: Option<&Url>,
    is_pending: bool,
) -> Result<Event, Error> {
    tracing::info!(
        "Inserting {} event: {:?}",
        if is_pending { "pending" } else { "confirmed" },
        event
    );

    let mut db_event = if is_pending {
        DbEvent::pending_event(event)?
    } else {
        DbEvent::confirmed_event(event)?
    };

    let (row_id, rows_changed) = DbEvent::insert(pool, &db_event).await?;
    db_event = db_event.with_id(row_id);

    if let Some(url) = relay_url {
        let _ev = relay_response_ok(pool, &db_event, url).await?;
    }

    if rows_changed == 0 {
        return Ok(Event::None);
    }

    match insert_specific_kind(pool, keys, relay_url, &db_event).await? {
        Some(has_event) => Ok(has_event),
        None => {
            if is_pending {
                Ok(Event::LocalPendingEvent(db_event))
            } else {
                Ok(Event::EventInserted(db_event))
            }
        }
    }
}

pub async fn insert_pending_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
) -> Result<Event, Error> {
    handle_insert_event(pool, keys, event, None, true).await
}

async fn insert_event(
    pool: &SqlitePool,
    keys: &Keys,
    event: nostr_sdk::Event,
    relay_url: &Url,
) -> Result<Event, Error> {
    handle_insert_event(pool, keys, event, Some(relay_url), false).await
}

#[derive(Debug, Clone)]
pub enum Event {
    DatabaseProcessing,
    DatabaseFinishedProcessing,
    LocalPendingEvent(DbEvent),
    /// Event triggered when a connection to the back-end is established
    DbConnected,
    /// Event triggered when the connection to the back-end is lost
    DbDisconnected,
    /// Event triggered when an error occurs
    Error(String),

    /// Event triggered when a list of chat messages is received
    GotChatMessages((DbContact, Vec<ChatMessage>)),
    GotRelayResponses(Vec<DbRelayResponse>),
    /// Event triggered when a list of contacts is received
    GotContacts(Vec<DbContact>),
    RelayCreated,
    RelayUpdated,
    RelayDeleted,
    ContactCreated(DbContact),
    ContactUpdated(DbContact),
    ContactDeleted(DbContact),
    ContactsImported(Vec<DbContact>),
    EventInserted(DbEvent),
    ReceivedDM((DbContact, ChatMessage)),
    NewDMAndContact((DbContact, ChatMessage)),
    UpdateWithRelayResponse {
        relay_response: DbRelayResponse,
        db_event: DbEvent,
        db_message: Option<DbMessage>,
    },
    None,
}

#[derive(Debug, Clone)]
pub enum Message {
    ProcessDatabaseMessages,
    FetchRelayResponses(i64),
    FetchMessages(DbContact),
    AddContact(DbContact),
    ImportContacts(Vec<DbContact>),
    FetchContacts,
    UpdateContact(DbContact),
    DeleteContact(DbContact),
    AddToUnseenCount(DbContact),
}

const IN_MEMORY: bool = false;
