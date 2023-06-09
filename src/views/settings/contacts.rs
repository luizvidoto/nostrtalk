use iced::widget::{button, column, container, row, text, text_input, tooltip, Space};
use iced::{Alignment, Length};

use crate::components::{common_scrollable, contact_row, ContactRow};
use crate::db::{DbRelay, DbRelayResponse};
use crate::error::BackendClosed;
use crate::icon::{import_icon, plus_icon, satellite_icon};
use crate::net::{self, BackEndConnection, BackendEvent};
use crate::style;
use crate::utils::contact_matches_search_full;
use crate::views::GoToView;
use crate::widget::Element;
use crate::{components::text::title, db::DbContact};

use super::SettingsRouterMessage;

#[derive(Debug, Clone)]
pub enum Message {
    DeleteContact(DbContact),
    OpenProfileModal(DbContact),
    ContactRow(contact_row::Message),
    OpenAddContactModal,
    OpenImportContactModal,
    SearchContactInputChange(String),
    RelaysConfirmationPress(Option<ContactsRelaysResponse>),
    SendDMTo(DbContact),
}

#[derive(Debug, Clone)]
pub struct ContactsRelaysResponse {
    pub confirmed_relays: Vec<DbRelayResponse>,
    pub all_relays: Vec<DbRelay>,
}
impl ContactsRelaysResponse {
    fn new(
        confirmed_relays: Vec<DbRelayResponse>,
        all_relays: Vec<DbRelay>,
    ) -> ContactsRelaysResponse {
        Self {
            confirmed_relays,
            all_relays,
        }
    }
}

pub struct State {
    contacts: Vec<DbContact>,
    search_contact_input: String,
    relays_response: Option<ContactsRelaysResponse>,
}
impl State {
    pub fn new(conn: &mut BackEndConnection) -> Result<Self, BackendClosed> {
        conn.send(net::ToBackend::FetchContacts)?;
        conn.send(net::ToBackend::FetchRelayResponsesContactList)?;
        Ok(Self {
            contacts: vec![],
            search_contact_input: "".into(),
            relays_response: None,
        })
    }

    pub fn backend_event(
        &mut self,
        event: BackendEvent,
        conn: &mut BackEndConnection,
    ) -> Result<(), BackendClosed> {
        match event {
            BackendEvent::ConfirmedContactList(_) => {
                conn.send(net::ToBackend::FetchRelayResponsesContactList)?;
            }
            BackendEvent::GotRelayResponsesContactList {
                responses,
                all_relays,
            } => {
                self.relays_response = Some(ContactsRelaysResponse::new(responses, all_relays));
            }
            BackendEvent::GotContacts(db_contacts) => {
                self.contacts = db_contacts;
            }
            BackendEvent::UpdatedMetadata(pubkey) => {
                if self.contacts.iter().any(|c| c.pubkey() == &pubkey) {
                    conn.send(net::ToBackend::FetchContactWithMetadata(pubkey))?;
                }
            }
            BackendEvent::GotSingleContact(_pubkey, Some(db_contact)) => {
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|c| c.pubkey() == db_contact.pubkey())
                {
                    *contact = db_contact;
                } else {
                    tracing::info!(
                        "GotSingleContact for someone not in the list?? {:?}",
                        db_contact
                    );
                }
            }
            BackendEvent::ReceivedContactList
            | BackendEvent::FileContactsImported(_)
            | BackendEvent::ContactCreated(_)
            | BackendEvent::ContactUpdated(_)
            | BackendEvent::ContactDeleted(_) => {
                conn.send(net::ToBackend::FetchContacts)?;
            }
            _ => (),
        }
        Ok(())
    }

    pub fn update(
        &mut self,
        message: Message,
        conn: &mut BackEndConnection,
    ) -> Result<Option<SettingsRouterMessage>, BackendClosed> {
        match message {
            Message::SendDMTo(_) => (),
            Message::RelaysConfirmationPress(_) => (),
            Message::OpenProfileModal(db_contact) => {
                return Ok(Some(SettingsRouterMessage::OpenProfileModal(db_contact)));
            }
            Message::OpenAddContactModal => {
                return Ok(Some(SettingsRouterMessage::OpenAddContactModal));
            }
            Message::OpenImportContactModal => {
                return Ok(Some(SettingsRouterMessage::OpenImportContactModal));
            }
            Message::SearchContactInputChange(text) => self.search_contact_input = text,
            Message::ContactRow(ct_msg) => match ct_msg {
                // TODO: dont return a message, find a better way
                contact_row::Message::SendMessageTo(contact) => {
                    return Ok(Some(SettingsRouterMessage::RouterMessage(
                        GoToView::ChatTo(contact),
                    )));
                }
                contact_row::Message::DeleteContact(contact) => {
                    conn.send(net::ToBackend::DeleteContact(contact))?;
                }
                contact_row::Message::EditContact(contact) => {
                    return Ok(Some(SettingsRouterMessage::OpenEditContactModal(contact)));
                }
            },
            Message::DeleteContact(contact) => {
                conn.send(net::ToBackend::DeleteContact(contact))?;
            }
        }

        Ok(None)
    }

    fn make_relays_response<'a>(&self) -> Element<'a, Message> {
        if let Some(response) = &self.relays_response {
            let resp_txt = format!(
                "{}/{}",
                &response.confirmed_relays.len(),
                &response.all_relays.len()
            );
            tooltip(
                button(
                    row![
                        text(&resp_txt).size(18).style(style::Text::Placeholder),
                        satellite_icon().size(16).style(style::Text::Placeholder)
                    ]
                    .spacing(5),
                )
                .on_press(Message::RelaysConfirmationPress(
                    self.relays_response.to_owned(),
                ))
                .style(style::Button::MenuBtn)
                .padding(5),
                "Contact List Confirmations",
                tooltip::Position::Left,
            )
            .style(style::Container::TooltipBg)
            .into()
        } else {
            text("Contact list not confirmed on any relays")
                .size(18)
                .style(style::Text::Placeholder)
                .into()
        }
    }

    pub fn view(&self) -> Element<Message> {
        let title = title("Contacts");
        let title_group = row![
            title,
            Space::with_width(Length::Fill),
            self.make_relays_response()
        ]
        .align_items(Alignment::Center)
        .padding([20, 20, 0, 0])
        .spacing(10);

        let search_contact = text_input("Search", &self.search_contact_input)
            .on_input(Message::SearchContactInputChange)
            .style(style::TextInput::ChatSearch)
            .width(SEARCH_CONTACT_WIDTH);
        let add_contact_btn = tooltip(
            button(
                row![text("Add").size(18), plus_icon().size(14)]
                    .align_items(Alignment::Center)
                    .spacing(2),
            )
            .padding(5)
            .on_press(Message::OpenAddContactModal),
            "Add Contact",
            tooltip::Position::Top,
        )
        .style(style::Container::TooltipBg);

        let import_btn = tooltip(
            button(import_icon().size(18))
                .padding(5)
                .on_press(Message::OpenImportContactModal),
            "Import from file",
            tooltip::Position::Top,
        )
        .style(style::Container::TooltipBg);

        let utils_row = row![
            search_contact,
            Space::with_width(Length::Fill),
            add_contact_btn,
            import_btn,
        ]
        .padding([0, 20, 0, 0])
        .spacing(5)
        .width(Length::Fill);

        let contact_list: Element<_> = self
            .contacts
            .iter()
            .filter(|c| contact_matches_search_full(c, &self.search_contact_input))
            .map(ContactRow::from_db_contact)
            .fold(
                column![].padding([0, 20, 0, 0]).spacing(5),
                |col, contact| col.push(contact.view().map(Message::ContactRow)),
            )
            .into();
        let contact_list_scroller = column![
            container(ContactRow::header()).padding([0, 20, 0, 0]),
            common_scrollable(contact_list)
        ];
        let content: Element<_> = column![title_group, utils_row, contact_list_scroller]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        container(content).center_x().center_y().into()
    }
}

const SEARCH_CONTACT_WIDTH: f32 = 200.0;
