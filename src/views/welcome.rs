use iced::alignment::Horizontal;
use iced::widget::{button, column, container, image, row, text, Space};
use iced::{Alignment, Length, Subscription};
use iced_aw::{Card, Modal};

use crate::components::text_input_group::TextInputGroup;
use crate::components::{common_scrollable, inform_card, relay_row, RelayRow};
use crate::consts::{NOSTR_RESOURCES_LINK, RELAYS_IMAGE, RELAY_SUGGESTIONS, WELCOME_IMAGE};
use crate::error::BackendClosed;
use crate::icon::{regular_circle_icon, solid_circle_icon};
use crate::net::{BackEndConnection, BackendEvent, ToBackend};
use crate::style;
use crate::{components::text::title, widget::Element};

use std::time::Duration;

use super::route::Route;
use super::{GoToView, RouterCommand};

#[derive(Debug, Clone)]
pub enum Message {
    RelayRow(Box<relay_row::MessageWrapper>),
    ToNextStep,
    ToPreviousStep,
    Logout,
    AddRelay(nostr::Url),
    AddOtherPress,

    // Add Relay Modal
    AddRelayInputChange(String),
    AddRelaySubmit(String),
    AddRelayCancelButtonPressed,
    CloseAddRelayModal,
    OpenLink(&'static str),
    AddAllRelays,
    Tick,
}

pub enum ModalState {
    AddRelay { relay_url: String, is_invalid: bool },
    Off,
}

impl ModalState {
    pub fn view<'a>(&'a self, underlay: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        match self {
            ModalState::AddRelay {
                relay_url,
                is_invalid,
            } => Modal::new(true, underlay.into(), move || {
                let mut add_relay_input =
                    TextInputGroup::new("Relay Address", relay_url, Message::AddRelayInputChange)
                        .placeholder("wss://my-relay.com")
                        .on_submit(Message::AddRelaySubmit(relay_url.clone()));

                if *is_invalid {
                    add_relay_input = add_relay_input.invalid("Relay address is invalid");
                }

                let modal_body: Element<_> = container(add_relay_input.build()).into();
                Card::new(text("Add Relay"), modal_body)
                    .foot(
                        row![
                            button(text("Cancel").horizontal_alignment(Horizontal::Center),)
                                .width(Length::Fill)
                                .on_press(Message::AddRelayCancelButtonPressed),
                            button(text("Ok").horizontal_alignment(Horizontal::Center),)
                                .width(Length::Fill)
                                .on_press(Message::AddRelaySubmit(relay_url.clone()))
                        ]
                        .spacing(10)
                        .padding(5)
                        .width(Length::Fill),
                    )
                    .max_width(CARD_MAX_WIDTH)
                    .on_close(Message::CloseAddRelayModal)
                    .into()
            })
            .backdrop(Message::CloseAddRelayModal)
            .on_esc(Message::CloseAddRelayModal)
            .into(),
            ModalState::Off => underlay.into(),
        }
    }

    fn open(&mut self) {
        *self = Self::AddRelay {
            relay_url: "".into(),
            is_invalid: false,
        }
    }

    fn close(&mut self) {
        *self = Self::Off
    }

    fn input_change(&mut self, input: String) {
        if let Self::AddRelay {
            relay_url,
            is_invalid,
        } = self
        {
            *relay_url = input;
            *is_invalid = false;
        }
    }

    fn error(&mut self) {
        if let Self::AddRelay { is_invalid, .. } = self {
            *is_invalid = true;
        }
    }
}

pub enum StepView {
    Welcome,
    Relays {
        relays_suggestion: Vec<nostr::Url>,
        relays_added: Vec<RelayRow>,
        add_relay_modal: ModalState,
    },
    LoadingClient,
}
impl StepView {
    fn relays_view(conn: &mut BackEndConnection) -> Result<Self, BackendClosed> {
        conn.send(ToBackend::FetchRelays)?;

        let relays_suggestion: Vec<_> = RELAY_SUGGESTIONS
            .iter()
            .filter_map(|s| nostr::Url::parse(s).ok())
            .collect();

        Ok(Self::Relays {
            relays_suggestion,
            relays_added: vec![],
            add_relay_modal: ModalState::Off,
        })
    }
    fn loading_client(conn: &mut BackEndConnection) -> Result<StepView, BackendClosed> {
        conn.send(ToBackend::PrepareClient)?;
        Ok(Self::LoadingClient)
    }
    fn get_step(&self) -> u8 {
        match self {
            StepView::Welcome => 1,
            StepView::Relays { .. } => 2,
            // StepView::DownloadEvents { .. } => 3,
            StepView::LoadingClient => 3,
        }
    }
    const MAX_STEP: u8 = 3;
    fn make_dots(&self) -> Element<'static, Message> {
        let step = self.get_step();
        let mut dot_row = row![].spacing(5);

        for i in 0..Self::MAX_STEP {
            if i < step {
                dot_row = dot_row.push(solid_circle_icon().size(10));
            } else {
                dot_row = dot_row.push(regular_circle_icon().size(10));
            }
        }

        dot_row.width(Length::Shrink).into()
    }

    fn make_btns(&self) -> Element<'static, Message> {
        match self {
            StepView::Welcome => row![
                button("Cancel").on_press(Message::Logout),
                button("Next").on_press(Message::ToNextStep)
            ]
            .spacing(10)
            .into(),
            StepView::Relays { .. } => row![
                button("Back").on_press(Message::ToPreviousStep),
                button("Start").on_press(Message::ToNextStep)
            ]
            .spacing(10)
            .into(),
            Self::LoadingClient => text("").into(),
        }
    }

    fn make_step_buttons(&self) -> Element<'static, Message> {
        column![
            container(self.make_dots()).center_x().width(Length::Fill),
            container(self.make_btns()).center_x().width(Length::Fill)
        ]
        .spacing(5)
        .into()
    }
    pub fn view(&self) -> Element<Message> {
        let welcome_image = image::Image::new(image::Handle::from_memory(WELCOME_IMAGE));
        let relays_image = image::Image::new(image::Handle::from_memory(RELAYS_IMAGE));
        // let contacts_image = image::Image::new(image::Handle::from_memory(CONTACTS_IMAGE));

        match self {
            StepView::Welcome => {
                let title_1 = "NostrTalk";
                let text_1a = "Secure, encrypted chats on the NOSTR network";
                let text_2a = "But what is NOSTR?";
                let text_2b = "It is the simplest open protocol that is able to create a censorship-resistant global “social” network once and for all";
                let text_3a =
                    "- It doesn’t rely on any trusted central server, hence it is resilient.";
                let text_3b =
                    "- It is based on cryptographic keys and signatures, so it is tamperproof.";
                let text_3c = "- It does not rely on P2P techniques, therefore it works.";

                let text_link = "Find more at: ";
                let nostr_link = button(text(NOSTR_RESOURCES_LINK).size(TEXT_SIZE_SMALL))
                    .padding(0)
                    .style(style::Button::Link)
                    .on_press(Message::OpenLink(NOSTR_RESOURCES_LINK));
                let nostr_link_group = row![text(text_link).size(TEXT_SIZE_SMALL), nostr_link]
                    .align_items(Alignment::Center);

                let content = column![
                    title(title_1)
                        .height(Length::FillPortion(1))
                        .width(Length::Fill)
                        .center_x()
                        .center_y(),
                    container(
                        row![
                            container(welcome_image)
                                .max_width(WELCOME_IMAGE_MAX_WIDTH)
                                .height(Length::Fill),
                            container(
                                column![
                                    text(text_1a).size(TEXT_SIZE_BIG),
                                    text(text_2a).size(TEXT_SIZE_BIG),
                                    text(text_2b).size(TEXT_SIZE_LARGE),
                                    column![
                                        text(text_3a).size(TEXT_SIZE_MEDIUM),
                                        text(text_3b).size(TEXT_SIZE_MEDIUM),
                                        text(text_3c).size(TEXT_SIZE_MEDIUM),
                                    ]
                                    .spacing(5),
                                    nostr_link_group
                                ]
                                .spacing(10)
                            )
                            .width(TEXT_WIDTH)
                            .height(Length::Fill)
                            .center_x()
                            .center_y(),
                        ]
                        .spacing(20)
                    )
                    .height(Length::FillPortion(4))
                    .width(Length::Fill)
                    .center_y()
                    .center_x(),
                    container(self.make_step_buttons()).height(Length::FillPortion(1))
                ]
                .spacing(30);

                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .center_y()
                    .style(style::Container::WelcomeBg1)
                    .into()
            }
            StepView::Relays {
                relays_added,
                relays_suggestion,
                add_relay_modal,
            } => {
                let title_2 = "Relays Setup";
                let text_2 = "Add relays to connect";
                let relays_suggestion =
                    relays_suggestion
                        .iter()
                        .fold(column![].spacing(5), |column, url| {
                            column.push(
                                container(
                                    row![
                                        text(url).size(20).width(Length::Fill),
                                        button("Add").on_press(Message::AddRelay(url.clone()))
                                    ]
                                    .align_items(Alignment::Center),
                                )
                                .width(Length::Fill)
                                .height(Length::Shrink),
                            )
                        });
                let relay_rows = relays_added
                    .iter()
                    .fold(column![].spacing(5), |col, relay| {
                        col.push(
                            relay
                                .relay_welcome()
                                .map(|m| Message::RelayRow(Box::new(m))),
                        )
                    });
                let add_other_btn = container(
                    button("Add Other")
                        .padding(10)
                        .on_press(Message::AddOtherPress),
                )
                .width(Length::Fill)
                .center_x()
                .center_y();
                let relays = container(common_scrollable(
                    column![relay_rows, relays_suggestion, add_other_btn]
                        .spacing(10)
                        .padding(20),
                ))
                .padding(5)
                .style(style::Container::Bordered)
                .width(Length::Fill)
                .height(Length::Fill);

                let add_all_btn = button("Add All")
                    .padding(5)
                    .style(style::Button::Primary)
                    .on_press(Message::AddAllRelays);

                let content = column![
                    title(title_2)
                        .height(Length::FillPortion(1))
                        .width(Length::Fill)
                        .center_x()
                        .center_y(),
                    container(
                        row![
                            container(relays_image)
                                .max_width(WELCOME_IMAGE_MAX_WIDTH)
                                .height(Length::Fill),
                            container(
                                column![
                                    container(
                                        row![
                                            text(text_2).size(TEXT_SIZE_LARGE),
                                            Space::with_width(Length::Fill),
                                            add_all_btn
                                        ]
                                        .align_items(Alignment::Center)
                                    )
                                    .padding(10)
                                    .width(Length::Fill),
                                    relays
                                ]
                                .spacing(10)
                            )
                            .width(Length::Fixed(TEXT_WIDTH))
                            .height(Length::Fill),
                        ]
                        .spacing(20)
                    )
                    .height(Length::FillPortion(4))
                    .width(Length::Fill)
                    .center_y()
                    .center_x(),
                    container(column![self.make_step_buttons()].spacing(5))
                        .height(Length::FillPortion(1))
                ]
                .spacing(10);

                let underlay = container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .center_y()
                    .style(style::Container::WelcomeBg2);

                add_relay_modal.view(underlay)
            }

            StepView::LoadingClient => inform_card("Loading", "Please wait..."),
        }
    }
}
pub struct State {
    pub step_view: StepView,
}
impl State {
    pub fn new() -> Self {
        Self {
            step_view: StepView::Welcome,
        }
    }
    fn next_step(&mut self, conn: &mut BackEndConnection) -> Result<(), BackendClosed> {
        match &self.step_view {
            StepView::Welcome => {
                self.step_view = StepView::relays_view(conn)?;
            }
            StepView::Relays { .. } => {
                self.step_view = StepView::loading_client(conn)?;
            }
            StepView::LoadingClient => {}
        }
        Ok(())
    }
    fn previous_step(&mut self, _conn: &mut BackEndConnection) {
        match &self.step_view {
            StepView::Welcome => {}
            StepView::Relays { .. } => self.step_view = StepView::Welcome,
            // StepView::DownloadEvents { .. } => {}
            StepView::LoadingClient => {}
        }
    }
}

impl Route for State {
    type Message = Message;
    fn subscription(&self) -> Subscription<Self::Message> {
        match &self.step_view {
            StepView::Relays { relays_added, .. } => {
                if relays_added.is_empty() {
                    iced::Subscription::none()
                } else {
                    iced::time::every(Duration::from_millis(TICK_INTERVAL_MILLIS))
                        .map(|_| Message::Tick)
                }
            }
            _ => iced::Subscription::none(),
        }
    }
    fn update(
        &mut self,
        message: Message,
        conn: &mut BackEndConnection,
    ) -> Result<RouterCommand<Self::Message>, BackendClosed> {
        let mut command = RouterCommand::new();

        match message {
            Message::Tick => {
                conn.send(ToBackend::GetRelayInformation)?;
            }
            Message::OpenLink(url) => {
                if let Err(e) = webbrowser::open(url) {
                    tracing::error!("Failed to open link: {}", e);
                }
            }
            Message::RelayRow(msg) => {
                if let StepView::Relays { relays_added, .. } = &mut self.step_view {
                    if let Some(row) = relays_added.iter_mut().find(|r| r.id == msg.from) {
                        let _ = row.update(msg.message, conn)?;
                    }
                }
            }
            Message::Logout => {
                conn.send(ToBackend::Logout)?;
                command.change_route(GoToView::Logout);
            }
            Message::ToNextStep => {
                self.next_step(conn)?;
            }
            Message::ToPreviousStep => self.previous_step(conn),
            Message::AddRelay(relay_url) => {
                if let StepView::Relays { .. } = &mut self.step_view {
                    conn.send(ToBackend::AddRelay(relay_url))?;
                }
            }
            Message::AddAllRelays => {
                if let StepView::Relays { .. } = &mut self.step_view {
                    for relay_url in RELAY_SUGGESTIONS {
                        if let Ok(relay_url) = nostr::Url::parse(relay_url) {
                            conn.send(ToBackend::AddRelay(relay_url))?;
                        }
                    }
                }
            }
            Message::AddOtherPress => {
                if let StepView::Relays {
                    add_relay_modal, ..
                } = &mut self.step_view
                {
                    add_relay_modal.open();
                }
            }
            Message::AddRelayCancelButtonPressed => {
                if let StepView::Relays {
                    add_relay_modal, ..
                } = &mut self.step_view
                {
                    add_relay_modal.close();
                }
            }
            Message::AddRelayInputChange(input) => {
                if let StepView::Relays {
                    add_relay_modal, ..
                } = &mut self.step_view
                {
                    add_relay_modal.input_change(input);
                }
            }
            Message::AddRelaySubmit(relay_url) => {
                if let StepView::Relays {
                    add_relay_modal, ..
                } = &mut self.step_view
                {
                    match nostr::Url::parse(&relay_url) {
                        Ok(url) => {
                            conn.send(ToBackend::AddRelay(url))?;
                            add_relay_modal.close();
                        }
                        Err(_e) => add_relay_modal.error(),
                    }
                }
            }
            Message::CloseAddRelayModal => {
                if let StepView::Relays {
                    add_relay_modal, ..
                } = &mut self.step_view
                {
                    add_relay_modal.close();
                }
            }
        }
        Ok(command)
    }

    fn backend_event(
        &mut self,
        event: BackendEvent,
        _conn: &mut BackEndConnection,
    ) -> Result<RouterCommand<Self::Message>, BackendClosed> {
        let mut command = RouterCommand::new();

        match &mut self.step_view {
            StepView::Relays {
                relays_added,
                relays_suggestion,
                ..
            } => match event {
                BackendEvent::RelayUpdated(db_relay) => {
                    if let Some(row) = relays_added
                        .iter_mut()
                        .find(|row| row.db_relay.url == db_relay.url)
                    {
                        row.relay_updated(db_relay);
                    } else {
                        tracing::warn!("Got information for unknown relay: {}", db_relay.url);
                    }
                }
                BackendEvent::GotRelays(mut db_relays) => {
                    for db_relay in db_relays.iter() {
                        relays_suggestion.retain(|url| url != &db_relay.url);
                    }
                    db_relays.sort_by(|a, b| a.url.cmp(&b.url));
                    *relays_added = db_relays
                        .into_iter()
                        .enumerate()
                        .map(|(idx, db_relay)| RelayRow::new(idx as i32, db_relay))
                        .collect();
                }
                BackendEvent::RelayCreated(db_relay) => {
                    relays_suggestion.retain(|url| url != &db_relay.url);
                    relays_added.push(RelayRow::new(relays_added.len() as i32, db_relay));
                }
                BackendEvent::RelayDeleted(url) => {
                    relays_added.retain(|row| row.db_relay.url != url);
                    relays_suggestion.push(url);
                }
                _ => (),
            },
            StepView::Welcome => (),
            StepView::LoadingClient => {
                if let BackendEvent::FinishedPreparing = event {
                    command.change_route(GoToView::Chat);
                }
            }
        }

        Ok(command)
    }

    fn view(&self, _selected_theme: Option<style::Theme>) -> Element<Self::Message> {
        self.step_view.view()
    }
}

const WELCOME_IMAGE_MAX_WIDTH: f32 = 300.0;
const TEXT_SIZE_BIG: u16 = 28;
const TEXT_SIZE_LARGE: u16 = 24;
const TEXT_SIZE_MEDIUM: u16 = 20;
const TEXT_SIZE_SMALL: u16 = 16;
const TEXT_WIDTH: f32 = 400.0;
const CARD_MAX_WIDTH: f32 = 300.0;
const TICK_INTERVAL_MILLIS: u64 = 500;
