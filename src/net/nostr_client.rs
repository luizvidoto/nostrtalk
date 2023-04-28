use crate::db::DbContact;
use crate::error::Error;
use crate::net::relay::{
    add_relay, connect_relay, connect_relays, fetch_relays, fetch_relays_urls,
    toggle_read_for_relay, toggle_write_for_relay, update_relay_db_and_client,
};
use crate::net::Connection;
use async_stream::stream;
use futures::Future;
use iced::futures::stream::Fuse;
use iced::futures::{channel::mpsc, StreamExt};
use iced::{subscription, Subscription};
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{
    Client, Contact, EventBuilder, EventId, Keys, Metadata, Relay, RelayMessage,
    RelayPoolNotification, Url,
};
use std::pin::Pin;
use std::str::FromStr;

use super::BackEndConnection;

pub enum State {
    Disconnected {
        keys: Keys,
        ns_conn: BackEndConnection<Message>,
    },
    Connected {
        ns_conn: BackEndConnection<Message>,
        receiver: mpsc::UnboundedReceiver<Message>,
        nostr_client: Client,
        keys: Keys,
        notifications_stream:
            Fuse<Pin<Box<dyn futures::Stream<Item = RelayPoolNotification> + Send>>>,
    },
}

pub fn nostr_client_connect(
    keys: &Keys,
    ns_conn: &BackEndConnection<Message>,
) -> Subscription<Event> {
    struct NostrClient;
    let id = std::any::TypeId::of::<NostrClient>();

    subscription::unfold(
        id,
        State::Disconnected {
            keys: keys.clone(),
            ns_conn: ns_conn.clone(),
        },
        |state| async move {
            match state {
                State::Disconnected { keys, mut ns_conn } => {
                    // Create new client
                    tracing::info!("Creating Nostr Client");
                    let nostr_client = Client::new(&keys);

                    let mut notifications = nostr_client.notifications();
                    let notifications_stream = stream! {
                        while let Ok(notification) = notifications.recv().await {
                            yield notification;
                        }
                    }
                    .boxed()
                    .fuse();

                    let (sender, receiver) = mpsc::unbounded();

                    ns_conn.with_channel(sender);

                    (
                        Event::NostrConnected,
                        State::Connected {
                            ns_conn,
                            receiver,
                            nostr_client,
                            keys,
                            notifications_stream,
                        },
                    )
                }
                State::Connected {
                    mut receiver,
                    nostr_client,
                    keys,
                    mut notifications_stream,
                    ns_conn,
                } => {
                    // tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    let event = futures::select! {
                        message = receiver.select_next_some() => {
                            let event = match message {
                                Message::SendContactListToRelay((relay_url, list)) => {
                                    process_async_fn(
                                        send_contact_list_to(&keys, &nostr_client, relay_url, &list),
                                        |event| event
                                    ).await
                                }
                                Message::CreateChannel => {
                                    process_async_fn(
                                        create_channel(&nostr_client),
                                        |event| event
                                    ).await
                                }
                                Message::SendDMTo((contact, msg)) => {
                                    process_async_fn(
                                        send_dm(&nostr_client, &keys, &contact, &msg),
                                        |event| event,
                                    )
                                    .await
                                }
                                Message::ShowPublicKey => {
                                    let pb_key = keys.public_key();
                                    Event::GotPublicKey(pb_key)
                                }
                                Message::FetchRelays => {
                                    process_async_fn(
                                        fetch_relays(&nostr_client),
                                        |relays| Event::GotRelays(relays),
                                    )
                                    .await
                                }
                                Message::FetchRelaysUrls => {
                                    process_async_fn(
                                        fetch_relays_urls(&nostr_client),
                                        |relays| Event::GotRelaysUrls(relays),
                                    )
                                    .await
                                }
                                Message::ConnectToRelay(relay_url) => {
                                    process_async_fn(
                                        connect_relay(&nostr_client, &relay_url),
                                        |_| Event::RelayConnected(relay_url.clone())
                                    ).await
                                }
                                Message::DeleteRelay(relay_url) => {
                                    process_async_fn(
                                        // DbRelay::delete(&database.pool, &relay_url),
                                        nostr_client.remove_relay(relay_url.as_str()),
                                        |_| Event::RelayDeleted,
                                    )
                                    .await
                                }
                                Message::AddRelay(url) => {
                                    println!("Message::AddRelay");
                                    process_async_fn(
                                        add_relay(&nostr_client, &url),
                                        |_| Event::RelayCreated,
                                    )
                                    .await
                                }
                                Message::UpdateRelay(url) => {
                                    process_async_fn(
                                        update_relay_db_and_client(&nostr_client, &url),
                                        |_| Event::RelayUpdated,
                                    )
                                    .await
                                }
                                Message::ConnectRelays => {
                                    process_async_fn(
                                        connect_relays(&nostr_client, &keys, 0), //TODO: fetch from db
                                        |_| Event::RelaysConnected,
                                    )
                                    .await
                                }
                                Message::ToggleRelayRead((url, read)) => {
                                    process_async_fn(
                                        toggle_read_for_relay(&nostr_client, &url, read),
                                        |_| Event::RelayUpdated
                                    ).await
                                }
                                Message::ToggleRelayWrite((url, write)) => {
                                    process_async_fn(
                                        toggle_write_for_relay(&nostr_client, &url, write),
                                        |_| Event::RelayUpdated
                                    ).await
                                },
                            };

                            event
                        }
                        notification = notifications_stream.select_next_some() => {
                            let event = match notification {
                                RelayPoolNotification::Event(relay_url, event) => {
                                    // println!("*** NOTIFICATION ***");
                                    // dbg!(&event);
                                    // process_async_fn(
                                    //     received_event(&database.pool, &keys, event, &relay_url),
                                    //     |event| event
                                    // ).await
                                    Event::ReceivedEvent((relay_url, event))
                                },
                                RelayPoolNotification::Message(relay_url, msg) => {
                                    // process_async_fn(
                                    //     on_relay_message(&database.pool, &relay_url, &msg),
                                    //     |event| event
                                    // ).await
                                    Event::ReceivedRelayMessage((relay_url, msg))
                                }
                                RelayPoolNotification::Shutdown => {
                                    Event::Shutdown
                                }
                            };

                            event
                        },
                    };
                    (
                        event,
                        State::Connected {
                            receiver,
                            nostr_client,
                            keys,
                            notifications_stream,
                            ns_conn,
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

async fn create_channel(client: &Client) -> Result<Event, Error> {
    let metadata = Metadata::new()
        .about("Channel about cars")
        .display_name("Best Cars")
        .name("Best Cars")
        .banner(Url::from_str("https://picsum.photos/seed/picsum/800/300")?)
        .picture(Url::from_str("https://picsum.photos/seed/picsum/200/300")?)
        .website(Url::from_str("https://picsum.photos/seed/picsum/200/300")?);

    let event_id = client.new_channel(metadata).await?;
    Ok(Event::ChannelCreated(event_id))
}

pub async fn send_contact_list_to(
    // pool: &SqlitePool,
    keys: &Keys,
    client: &Client,
    url: Url,
    list: &[DbContact],
) -> Result<Event, Error> {
    // let list = DbContact::fetch(pool).await?;
    let c_list: Vec<Contact> = list.iter().map(|c| c.into()).collect();

    let builder = EventBuilder::set_contact_list(c_list);
    let event = builder.to_event(keys)?;

    let _event_id = client.send_event_to(url, event.clone()).await?;

    Ok(Event::InsertPendingEvent(event))
}

pub async fn send_dm(
    nostr_client: &Client,
    keys: &Keys,
    db_contact: &DbContact,
    content: &str,
) -> Result<Event, Error> {
    tracing::info!("Sending DM to relays");
    let mut has_event: Option<(nostr_sdk::Event, Url)> = None;
    let builder =
        EventBuilder::new_encrypted_direct_msg(&keys, db_contact.pubkey().to_owned(), content)?;
    let event = builder.to_event(keys)?;

    for (url, relay) in nostr_client.relays().await {
        if !relay.opts().write() {
            // return Err(Error::WriteActionsDisabled(url.clone()))
            tracing::error!("{}", Error::WriteActionsDisabled(url.to_string()));
            continue;
        }

        if let Ok(_id) = nostr_client.send_event_to(url.clone(), event.clone()).await {
            has_event = Some((event.clone(), url.clone()));
        }
    }

    if let Some((event, _relay_url)) = has_event {
        Ok(Event::InsertPendingEvent(event))
    } else {
        Err(Error::NoRelayToWrite)
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    InsertPendingEvent(nostr_sdk::Event),
    ReceivedEvent((Url, nostr_sdk::Event)),
    ReceivedRelayMessage((Url, nostr_sdk::RelayMessage)),
    /// Event triggered when a connection to the back-end is established
    NostrConnected,
    /// Event triggered when the connection to the back-end is lost
    NostrDisconnected,

    /// Event triggered when an error occurs
    Error(String),

    /// Event triggered when relays are connected
    RelaysConnected,
    /// Event triggered when a list of relays is received
    GotRelays(Vec<Relay>),
    GotRelaysUrls(Vec<nostr_sdk::Url>),
    /// Event triggered when a list of own events is received
    GotOwnEvents(Vec<nostr_sdk::Event>),
    /// Event triggered when a public key is received
    GotPublicKey(XOnlyPublicKey),
    /// Event triggered when a relay is removed
    RelayRemoved(String),
    /// Event triggered when a Nostr event is received
    NostrEvent(nostr_sdk::Event),
    /// Event triggered when a relay message is received
    RelayMessage(RelayMessage),
    /// Event triggered when the system is shutting down
    Shutdown,
    /// Event triggered when a relay is connected
    RelayConnected(Url),
    /// Event triggered when a relay is updated
    UpdatedRelay,

    ChannelCreated(EventId),

    RelayCreated,
    RelayUpdated,
    RelayDeleted,
}

#[derive(Debug, Clone)]
pub enum Message {
    ConnectRelays,
    ConnectToRelay(Url),
    SendDMTo((DbContact, String)),
    ShowPublicKey,
    FetchRelays,
    FetchRelaysUrls,
    AddRelay(Url),
    UpdateRelay(Url),
    DeleteRelay(Url),
    ToggleRelayRead((Url, bool)),
    ToggleRelayWrite((Url, bool)),
    SendContactListToRelay((Url, Vec<DbContact>)),
    CreateChannel,
}
