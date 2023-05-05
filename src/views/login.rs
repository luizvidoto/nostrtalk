use iced::{
    alignment,
    widget::{button, column, container, row, text, Space},
    Length,
};
use nostr_sdk::{prelude::FromSkStr, Keys};

use crate::{
    components::{text::title, text_input_group::TextInputGroup},
    style,
    widget::Element,
};

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub about: String,
    pub profile_picture: String,
}
impl Profile {
    pub fn new(name: String, about: String, profile_picture: String) -> Self {
        Self {
            name,
            about,
            profile_picture,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SecretKeyInputChange(String),
    SubmitPress(String),
    ToCreateAccount,
    ToImportAccount,
    ToChooseAccount,
    CreateAccountSubmit(Profile),
    NameInputChange(String),
    AboutInputChange(String),
    ProfilePictureInputChange(String),
    // to main
    CreateAccountSubmitSuccess((Profile, Keys)),
    LoginSuccess(Keys),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum State {
    ChooseAccount,
    CreateAccount {
        name: String,
        about: String,
        profile_picture: String,
    },
    ImportAccount {
        secret_key_input: String,
        is_invalid: bool,
    },
}
impl State {
    pub fn new() -> Self {
        Self::ChooseAccount
    }
    pub fn import_account() -> Self {
        Self::ImportAccount {
            secret_key_input: "4510459b74db68371be462f19ef4f7ef1e6c5a95b1d83a7adf00987c51ac56fe"
                .into(),
            is_invalid: false,
        }
    }
    pub fn create_account() -> Self {
        Self::CreateAccount {
            name: "".into(),
            about: "".into(),
            profile_picture: "".into(),
        }
    }

    pub fn update(&mut self, message: Message) -> Option<Message> {
        match self {
            State::ChooseAccount => match message {
                Message::ToCreateAccount => *self = Self::create_account(),
                Message::ToImportAccount => *self = Self::import_account(),
                _ => (),
            },
            State::CreateAccount {
                name: name_input,
                about: about_input,
                profile_picture: profile_picture_input,
            } => match message {
                Message::NameInputChange(text) => *name_input = text,
                Message::AboutInputChange(text) => *about_input = text,
                Message::ProfilePictureInputChange(text) => *profile_picture_input = text,
                Message::ToChooseAccount => *self = Self::new(),
                Message::CreateAccountSubmit(profile) => {
                    let keys = Keys::generate();
                    return Some(Message::CreateAccountSubmitSuccess((profile, keys)));
                }
                _ => (),
            },
            State::ImportAccount {
                secret_key_input,
                is_invalid,
            } => match message {
                Message::SecretKeyInputChange(secret_key) => {
                    *secret_key_input = secret_key;
                    *is_invalid = false;
                }
                Message::SubmitPress(secret_key) => match Keys::from_sk_str(&secret_key) {
                    Ok(keys) => return Some(Message::LoginSuccess(keys)),
                    Err(e) => {
                        tracing::error!("Invalid secret key: {}", e);
                        *is_invalid = true;
                    }
                },
                Message::ToChooseAccount => *self = Self::new(),
                _ => (),
            },
        }

        None
    }

    pub fn view(&self) -> Element<Message> {
        let content: Element<_> = match self {
            State::ChooseAccount => {
                let page_title = title("Sign In").center_x();
                let create_acc_btn = button(
                    container(
                        text("Create Nostr Account")
                            .horizontal_alignment(alignment::Horizontal::Center)
                            .vertical_alignment(alignment::Vertical::Center),
                    )
                    .center_x()
                    .center_y()
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .on_press(Message::ToCreateAccount)
                .width(Length::Fill)
                .height(Length::Fill);

                let import_acc_btn = button(
                    container(
                        text("Import With Keys")
                            .horizontal_alignment(alignment::Horizontal::Center)
                            .vertical_alignment(alignment::Vertical::Center),
                    )
                    .center_x()
                    .center_y()
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .on_press(Message::ToImportAccount)
                .width(Length::Fill)
                .height(Length::Fill);

                let buttons = row![create_acc_btn, import_acc_btn]
                    .height(100.0)
                    .spacing(20)
                    .width(Length::Fill);
                column![page_title, buttons]
                    .spacing(20)
                    .width(Length::Fill)
                    .into()
            }
            State::CreateAccount {
                name,
                about,
                profile_picture,
            } => {
                let name_input = TextInputGroup::new("Name", name, Message::NameInputChange);
                let about_input = TextInputGroup::new("About", about, Message::AboutInputChange);
                let profile_pic_input = TextInputGroup::new(
                    "Profile Picture",
                    profile_picture,
                    Message::ProfilePictureInputChange,
                );
                let back_btn = button("Back")
                    .style(style::Button::Invisible)
                    .on_press(Message::ToChooseAccount);
                let submit_btn = button("Submit").on_press(Message::CreateAccountSubmit(
                    Profile::new(name.clone(), about.clone(), profile_picture.clone()),
                ));
                let buttons =
                    row![back_btn, Space::with_width(Length::Fill), submit_btn].spacing(10);
                column![
                    title("Create Nostr Account"),
                    name_input.build(),
                    about_input.build(),
                    profile_pic_input.build(),
                    buttons
                ]
                .spacing(20)
                .into()
            }
            State::ImportAccount {
                secret_key_input,
                is_invalid,
            } => {
                let mut secret_input = TextInputGroup::new(
                    "Secret Key",
                    secret_key_input,
                    Message::SecretKeyInputChange,
                )
                .placeholder("a4s6d84as6d4a...")
                .on_submit(Message::SubmitPress(secret_key_input.clone()));

                if *is_invalid {
                    secret_input = secret_input.invalid("Invalid Secret Key");
                }

                let back_btn = button("Back")
                    .style(style::Button::Invisible)
                    .on_press(Message::ToChooseAccount);
                let submit_btn =
                    button("Submit").on_press(Message::SubmitPress(secret_key_input.clone()));
                let buttons =
                    row![back_btn, Space::with_width(Length::Fill), submit_btn].spacing(10);
                column![title("Login"), secret_input.build(), buttons]
                    .spacing(20)
                    .into()
            }
        };

        let form = container(content)
            .width(400.0)
            .padding(30)
            .style(style::Container::ContactList);

        container(form)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .style(style::Container::ChatContainer)
            .into()
    }
}
