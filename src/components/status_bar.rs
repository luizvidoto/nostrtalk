use iced::widget::{button, container, row, text, Space};
use iced::Subscription;
use iced::{alignment, Alignment, Command, Length};

use crate::consts::NOSTRTALK_VERSION;
use crate::error::BackendClosed;
use crate::icon::signal_icon;
use crate::net::{self, BackEndConnection, BackendEvent};
use crate::style;
use crate::views::{GoToView, RouterCommand};
use crate::widget::Element;

#[derive(Debug, Clone)]
pub enum Message {
    GoToAbout,
    GoToNetwork,
    Tick,
}
pub struct StatusBar {
    relays_connected: usize,
}
impl StatusBar {
    pub fn new() -> Self {
        Self {
            relays_connected: 0,
        }
    }
    pub fn backend_event(
        &mut self,
        event: BackendEvent,
        _conn: &mut BackEndConnection,
    ) -> Command<Message> {
        if let BackendEvent::GotRelayStatusList(list) = event {
            self.relays_connected = list
                .iter()
                .filter(|(_url, status)| status.is_connected())
                .count();
        }
        Command::none()
    }
    pub fn update(
        &mut self,
        message: Message,
        conn: &mut BackEndConnection,
    ) -> Result<RouterCommand<Message>, BackendClosed> {
        let mut command = RouterCommand::new();
        match message {
            Message::GoToAbout => command.change_route(GoToView::About),
            Message::GoToNetwork => command.change_route(GoToView::Network),
            Message::Tick => {
                conn.send(net::ToBackend::GetRelayStatusList)?;
            }
        }
        Ok(command)
    }
    pub fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(TICK_INTERVAL_MILLIS))
            .map(|_| Message::Tick)
    }
    pub fn view(&self) -> Element<'static, Message> {
        let about = button(text(format!("NostrTalk v{}", NOSTRTALK_VERSION)).size(18))
            .padding([0, 2])
            .height(Length::Fill)
            .on_press(Message::GoToAbout)
            .style(style::Button::StatusBarButton);
        let signal = button(
            row![text(self.relays_connected).size(18), signal_icon().size(12),]
                .align_items(Alignment::Center),
        )
        .height(Length::Fill)
        .padding([0, 2])
        .on_press(Message::GoToNetwork)
        .style(style::Button::StatusBarButton);

        container(
            row![about, Space::with_width(Length::Fill), signal].align_items(Alignment::Center),
        )
        .padding(0)
        .align_x(alignment::Horizontal::Right)
        .align_y(alignment::Vertical::Bottom)
        .height(Length::Fixed(STATUS_BAR_HEIGHT))
        .width(Length::Fill)
        .style(style::Container::StatusBar)
        .into()
    }
}

pub const STATUS_BAR_HEIGHT: f32 = 20.0;
const TICK_INTERVAL_MILLIS: u64 = 500;
