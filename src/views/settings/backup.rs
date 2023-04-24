use crate::{components::text::title, net, widget::Element};

#[derive(Debug, Clone)]
pub enum Message {
    BackEndEvent(net::Event),
}

#[derive(Debug, Clone)]
pub struct State {}
impl State {
    pub fn default() -> Self {
        Self {}
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::BackEndEvent(_ev) => (),
        }
    }

    pub fn view(&self) -> Element<Message> {
        title("Backup").into()
    }
}
