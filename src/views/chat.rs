use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::components;
use crate::components::chat_card::{self, ChatCard};
use crate::net::{self, Connection};

#[derive(Debug, Clone)]
pub enum Message {
    OnVerResize(u16),
    AddRelay,
    ShowRelays,
    NavSettingsPress,
    ChatCardMessage(components::chat_card::Message),
    GetOwnEvents,
}

#[derive(Debug, Clone)]
pub struct State {
    ver_divider_position: Option<u16>,
    chats: Vec<chat_card::State>,
}
impl State {
    pub fn new() -> Self {
        let mut chats: Vec<chat_card::State> = vec![];
        for id in 0..10 {
            chats.push(chat_card::State::new(ChatCard::new(id)));
        }
        Self {
            ver_divider_position: None,
            chats,
        }
    }
    pub fn view(&self) -> Element<Message> {
        let first = container(column![scrollable(
            self.chats.iter().fold(column![].spacing(0), |col, card| {
                col.push(card.view().map(Message::ChatCardMessage))
            })
        )])
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y();

        let add_relay_btn = button("Add Relay").on_press(Message::AddRelay);
        let show_relay_btn = button("Show Relay").on_press(Message::ShowRelays);
        let get_own_events_btn = button("Own Events").on_press(Message::GetOwnEvents);
        let second =
            container(column![add_relay_btn, show_relay_btn, get_own_events_btn].spacing(10))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x()
                .center_y();
        let content = iced_aw::split::Split::new(
            first,
            second,
            self.ver_divider_position,
            iced_aw::split::Axis::Vertical,
            Message::OnVerResize,
        );

        let search_input = container(text("Search")).padding(10);
        let settings_btn = button("Settings")
            .padding(10)
            .on_press(Message::NavSettingsPress);
        let empty = container(text("")).width(Length::Fill);
        let navbar = row![search_input, empty, settings_btn]
            .width(Length::Fill)
            .padding(10)
            .spacing(10);

        column![navbar, content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    pub fn update(&mut self, message: Message, conn: &mut Connection) {
        match message {
            Message::GetOwnEvents => {
                conn.send(net::Message::GetOwnEvents);
            }
            Message::OnVerResize(position) => {
                if position > 200 {
                    self.ver_divider_position = Some(position);
                } else if position <= 200 && position > 120 {
                    self.ver_divider_position = Some(200);
                    for c in &mut self.chats {
                        c.update(chat_card::Message::ShowFullCard);
                    }
                } else if position <= 120 {
                    self.ver_divider_position = Some(80);
                    for c in &mut self.chats {
                        c.update(chat_card::Message::ShowOnlyProfileImage);
                    }
                }
            }
            Message::NavSettingsPress => (),
            Message::AddRelay => {
                for r in vec![
                    "wss://eden.nostr.land",
                    "wss://nostr.fmt.wiz.biz",
                    "wss://relay.damus.io",
                    "wss://nostr.anchel.nl/",
                    // "ws://192.168.15.119:8080"
                ] {
                    if let Err(e) = conn.send(net::Message::AddRelay(r.into())) {
                        println!("{}", e);
                    }
                }
            }
            Message::ShowRelays => {
                if let Err(e) = conn.send(net::Message::ShowRelays) {
                    println!("{}", e);
                }
            }
            Message::ChatCardMessage(msg) => {
                for c in &mut self.chats {
                    c.update(msg.clone());
                }
            }
        }
    }
}
