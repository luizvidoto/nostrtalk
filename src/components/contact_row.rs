use iced::widget::{button, container, row, text, tooltip};
use iced::Length;
use nostr::prelude::ToBech32;

use crate::db::DbContact;
use crate::icon::{delete_icon, edit_icon, reply_icon};
use crate::style;
use crate::utils::hide_string;
use crate::widget::Element;

#[derive(Debug, Clone)]
pub enum Message {
    DeleteContact(DbContact),
    EditContact(DbContact),
    SendMessageTo(DbContact),
}
pub struct ContactRow {
    contact: DbContact,
    pubkey: String,
}

impl From<ContactRow> for DbContact {
    fn from(row: ContactRow) -> Self {
        row.contact
    }
}

impl From<&ContactRow> for DbContact {
    fn from(row: &ContactRow) -> Self {
        row.contact.to_owned()
    }
}

impl ContactRow {
    pub fn from_db_contact(db_contact: &DbContact) -> Self {
        Self {
            contact: db_contact.clone(),
            pubkey: db_contact
                .pubkey()
                .to_bech32()
                .unwrap_or(db_contact.pubkey().to_string()),
        }
    }
    pub fn header<M: 'static>() -> Element<'static, M> {
        row![
            container(text("Public Key")).width(Length::Fixed(PUBKEY_CELL_WIDTH)),
            container(text("PetName"))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text("Name"))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text("Username"))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text("Relay"))
                .align_x(iced::alignment::Horizontal::Left)
                .width(Length::Fill),
            container(text("")).width(Length::Fixed(EDIT_BTN_WIDTH)),
            container(text("")).width(Length::Fixed(REMOVE_BTN_WIDTH)),
        ]
        .spacing(2)
        .into()
    }
    pub fn view(&self) -> Element<'static, Message> {
        row![
            container(text(hide_string(&self.pubkey, 6))).width(Length::Fixed(PUBKEY_CELL_WIDTH)),
            container(text(&self.contact.get_petname().unwrap_or("".into())))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text(&self.contact.get_profile_name().unwrap_or("".into())))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text(&self.contact.get_display_name().unwrap_or("".into())))
                .width(Length::Fixed(NAME_CELL_WIDTH_MIN))
                .max_width(NAME_CELL_WIDTH_MAX),
            container(text(
                &self
                    .contact
                    .get_relay_url()
                    .map(|url| url.to_string())
                    .unwrap_or("".into())
            ))
            .width(Length::Fill),
            container(
                tooltip(
                    button(reply_icon().size(16)).on_press(Message::SendMessageTo(self.into())),
                    "Send Message",
                    tooltip::Position::Left
                )
                .style(style::Container::TooltipBg)
            )
            .width(Length::Fixed(EDIT_BTN_WIDTH)),
            container(
                tooltip(
                    button(edit_icon().size(16))
                        .on_press(Message::EditContact(self.into()))
                        .width(Length::Fixed(EDIT_BTN_WIDTH)),
                    "Edit Contact",
                    tooltip::Position::Left
                )
                .style(style::Container::TooltipBg)
            ),
            container(
                tooltip(
                    button(delete_icon().size(16))
                        .on_press(Message::DeleteContact(self.contact.clone()))
                        .style(style::Button::Danger),
                    "Delete Contact",
                    tooltip::Position::Left
                )
                .style(style::Container::TooltipBg)
            )
            .width(Length::Fixed(REMOVE_BTN_WIDTH))
        ]
        .spacing(2)
        .into()
    }
}

const EDIT_BTN_WIDTH: f32 = 30.0;
const REMOVE_BTN_WIDTH: f32 = 30.0;
const PUBKEY_CELL_WIDTH: f32 = 120.0;
const NAME_CELL_WIDTH_MIN: f32 = 100.0;
const NAME_CELL_WIDTH_MAX: f32 = 200.0;
