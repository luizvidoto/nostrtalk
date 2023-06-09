use iced::Color;

use super::Theme;

#[derive(Debug, Clone, Copy, Default)]
pub enum Modal {
    #[default]
    Default,
}
impl iced_aw::modal::StyleSheet for Theme {
    type Style = Modal;
    fn active(&self, style: Self::Style) -> iced_aw::style::modal::Appearance {
        match style {
            Modal::Default => {
                let mut background = Color::BLACK;
                background.a = 0.7;
                iced_aw::style::modal::Appearance {
                    background: background.into(),
                }
            }
        }
    }
}
