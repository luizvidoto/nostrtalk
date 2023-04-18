use iced::widget::{button, column, row, text};
use iced::{Color, Element, Length};
use nostr_sdk::secp256k1::XOnlyPublicKey;

use crate::db::DbContact;

#[derive(Debug, Clone)]
pub enum Message {
    UpdateActiveId(XOnlyPublicKey),
    ShowOnlyProfileImage,
    ShowFullCard,
}

#[derive(Debug, Clone)]
pub struct State {
    active_pubkey: Option<XOnlyPublicKey>,
    only_profile: bool,
    last_msg_date: Option<i64>,
    last_msg_snippet: Option<String>,
    contact: DbContact,
}

impl State {
    pub fn from_db_contact(db_contact: &DbContact) -> Self {
        Self {
            active_pubkey: None,
            only_profile: false,
            contact: db_contact.to_owned(),
            last_msg_date: None,
            last_msg_snippet: None,
        }
    }
    pub fn view(&self) -> Element<Message> {
        let mut is_active = false;
        if let Some(pubkey) = &self.active_pubkey {
            is_active = pubkey == &self.contact.pubkey;
        }
        let btn_style = if is_active {
            iced::theme::Button::Custom(Box::new(ActiveButtonStyle {}))
        } else {
            iced::theme::Button::Custom(Box::new(ButtonStyle {}))
        };
        let btn_content: Element<_> = if self.only_profile {
            text(&self.contact.profile_image.to_owned().unwrap_or("".into())).into()
        } else {
            row![
                text("Profile Image"),
                column![
                    text(&self.contact.petname.to_owned().unwrap_or("".into())),
                    text(&self.last_msg_snippet.to_owned().unwrap_or("".into()))
                        .size(14.0)
                        .width(Length::Fill)
                        .height(Length::Fixed(30.0)),
                ],
                text(&self.last_msg_date.unwrap_or(0)),
            ]
            .into()
        };
        button(btn_content)
            .width(Length::Fill)
            .height(Length::Fixed(80.0))
            .on_press(Message::UpdateActiveId(self.contact.pubkey.clone()))
            .style(btn_style)
            .into()
    }
    pub fn update(&mut self, message: Message) {
        match message {
            Message::UpdateActiveId(id) => {
                self.active_pubkey = Some(id);
            }
            Message::ShowOnlyProfileImage => {
                self.only_profile = true;
            }
            Message::ShowFullCard => {
                self.only_profile = false;
            }
        }
    }
}

struct ButtonStyle;
impl button::StyleSheet for ButtonStyle {
    type Style = iced::Theme;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        button::Appearance {
            text_color: style.extended_palette().background.base.text,
            border_radius: 0.0,
            background: Some(Color::TRANSPARENT.into()),
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let plt = style.extended_palette();

        button::Appearance {
            background: Some(Color::from_rgb8(240, 240, 240).into()),
            text_color: plt.primary.weak.text,
            ..self.active(style)
        }
    }
}

struct ActiveButtonStyle;
impl button::StyleSheet for ActiveButtonStyle {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> button::Appearance {
        button::Appearance {
            border_radius: 0.0,
            background: Some(Color::from_rgb8(65, 159, 217).into()),
            text_color: Color::WHITE,
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        button::Appearance {
            ..self.active(style)
        }
    }
}