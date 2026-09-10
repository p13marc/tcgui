//! A single reusable confirmation dialog for destructive actions.
//!
//! A caller declares that an action needs confirming by intercepting it in
//! `update` and calling [`ConfirmState::request`] with the message to replay on
//! acceptance. Nothing else changes: the widgets that emit those messages are
//! untouched, and an action is either confirmed and replayed verbatim or not
//! performed at all.
//!
//! **Intercept the entry message, not the effect message.** Several destructive
//! paths mutate local UI state on the way to issuing their backend task — for
//! example `ClearAllFeatures` wipes the feature checkboxes in
//! `TcInterface::update` *before* the `RemoveTc` task is produced. Gating the
//! effect would leave a cancelled dialog showing "cleared" while the kernel
//! still has netem on the interface.

use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Length};

use crate::messages::TcGuiMessage;
use crate::view::{ColorPalette, scaled, scaled_padding, scaled_spacing};

/// A pending confirmation: what to ask, and what to do if the answer is yes.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    /// Renders the confirm button in the danger colour.
    pub destructive: bool,
    /// Replayed verbatim when the user accepts. Boxed because `TcGuiMessage`
    /// would otherwise contain itself.
    pub action: Box<TcGuiMessage>,
}

impl ConfirmRequest {
    fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        confirm_label: impl Into<String>,
        action: TcGuiMessage,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            confirm_label: confirm_label.into(),
            destructive: true,
            action: Box::new(action),
        }
    }

    /// Clearing every netem feature on one interface.
    pub fn clear_all_features(interface: &str, action: TcGuiMessage) -> Self {
        Self::new(
            "Clear all traffic control?",
            format!(
                "Every netem feature on {interface} will be removed and the qdisc \
                 deleted. Traffic shaping stops immediately."
            ),
            "Clear all",
            action,
        )
    }

    /// Taking an interface administratively down.
    pub fn disable_interface(interface: &str, action: TcGuiMessage) -> Self {
        Self::new(
            "Take interface down?",
            format!(
                "{interface} will be brought administratively down. If you are \
                 connected through it, you will lose access to this host."
            ),
            "Take down",
            action,
        )
    }

    /// Installing a plug — the one plug verb that stops traffic.
    ///
    /// The release verbs are deliberately **not** gated: a confirmation dialog
    /// in front of the escape hatch is a bug, not a safety feature.
    pub fn plug_interface(interface: &str, action: TcGuiMessage) -> Self {
        Self::new(
            "Stall traffic on this interface?",
            format!(
                "{interface} will stop forwarding immediately and hold packets \
                 until you release it. If you are connected through it, you \
                 will lose access to this host until then."
            ),
            "Stall traffic",
            action,
        )
    }

    /// Stopping a running scenario mid-flight.
    pub fn stop_scenario(interface: &str, action: TcGuiMessage) -> Self {
        Self::new(
            "Stop the running scenario?",
            format!(
                "The scenario on {interface} will stop at its current step and its \
                 traffic control will be cleaned up."
            ),
            "Stop",
            action,
        )
    }

    /// Resetting every UI visibility toggle.
    pub fn reset_ui_state(action: TcGuiMessage) -> Self {
        let mut r = Self::new(
            "Reset the view?",
            "Every hidden backend and namespace becomes visible again and all \
             view toggles return to their defaults. No traffic control is changed."
                .to_string(),
            "Reset view",
            action,
        );
        // Nothing on the wire changes, so this one is not a danger action.
        r.destructive = false;
        r
    }
}

/// Holds at most one pending confirmation.
#[derive(Debug, Default)]
pub struct ConfirmState {
    pending: Option<ConfirmRequest>,
}

impl ConfirmState {
    /// Ask for confirmation. A second request while one is open is ignored, so
    /// a repeated key press cannot stack dialogs.
    pub fn request(&mut self, request: ConfirmRequest) {
        if self.pending.is_none() {
            self.pending = Some(request);
        }
    }

    /// Take the pending action, closing the dialog. Returns `None` if nothing
    /// was pending, so a stray `ConfirmAccepted` cannot replay an action twice.
    pub fn take(&mut self) -> Option<Box<TcGuiMessage>> {
        self.pending.take().map(|r| r.action)
    }

    /// Close the dialog without performing the action.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    pub fn pending(&self) -> Option<&ConfirmRequest> {
        self.pending.as_ref()
    }
}

/// The dimmed full-window backdrop shared by every modal overlay.
pub fn modal_backdrop<'a>(
    content: Element<'a, TcGuiMessage>,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    container(content)
        .padding(scaled_padding(40, zoom))
        .center(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.0, 0.0, 0.0, 0.5,
            ))),
            ..container::Style::default()
        })
        .into()
}

/// Render the confirmation dialog.
pub fn render_confirm<'a>(
    request: &'a ConfirmRequest,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let danger = colors.error_red;
    let accent = colors.primary_blue;
    let confirm_bg = if request.destructive { danger } else { accent };

    let card = container(
        column![
            text(&request.title)
                .size(scaled(18, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary),
                }),
            text(&request.body)
                .size(scaled(13, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                }),
            row![
                button(text("Cancel").size(scaled(13, zoom)))
                    .on_press(TcGuiMessage::ConfirmCancelled)
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(colors.background_card)),
                        text_color: colors.text_primary,
                        border: iced::Border {
                            radius: 6.0.into(),
                            width: 1.0,
                            color: colors.text_secondary,
                        },
                        ..button::Style::default()
                    }),
                button(text(&request.confirm_label).size(scaled(13, zoom)))
                    .on_press(TcGuiMessage::ConfirmAccepted)
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(confirm_bg)),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..iced::Border::default()
                        },
                        ..button::Style::default()
                    }),
            ]
            .spacing(scaled_spacing(12, zoom))
        ]
        .spacing(scaled_spacing(16, zoom)),
    )
    .padding(scaled_padding(24, zoom))
    .max_width(480)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(colors.background_card)),
        border: iced::Border {
            radius: 12.0.into(),
            width: 1.0,
            color: colors.text_secondary,
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 16.0,
        },
        ..container::Style::default()
    });

    modal_backdrop(card.into(), zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ConfirmRequest {
        ConfirmRequest::reset_ui_state(TcGuiMessage::ResetUiState)
    }

    #[test]
    fn take_yields_the_action_once_then_closes() {
        let mut state = ConfirmState::default();
        assert!(!state.is_open());
        state.request(req());
        assert!(state.is_open());

        let action = state.take().expect("pending action");
        assert!(matches!(*action, TcGuiMessage::ResetUiState));
        assert!(!state.is_open(), "dialog stays open after accepting");
        assert!(
            state.take().is_none(),
            "a second accept replayed the action"
        );
    }

    #[test]
    fn cancel_closes_without_yielding_the_action() {
        let mut state = ConfirmState::default();
        state.request(req());
        state.cancel();
        assert!(!state.is_open());
        assert!(state.take().is_none(), "cancel still performed the action");
    }

    #[test]
    fn a_second_request_does_not_stack() {
        let mut state = ConfirmState::default();
        state.request(ConfirmRequest::clear_all_features(
            "eth0",
            TcGuiMessage::ResetUiState,
        ));
        state.request(ConfirmRequest::disable_interface(
            "eth1",
            TcGuiMessage::ShowAllBackends,
        ));
        assert_eq!(state.pending().unwrap().title, "Clear all traffic control?");
    }

    /// The dialog must name the interface — "are you sure?" with no subject is
    /// how people confirm the wrong thing.
    #[test]
    fn destructive_prompts_name_their_target() {
        for r in [
            ConfirmRequest::clear_all_features("eth0", TcGuiMessage::ResetUiState),
            ConfirmRequest::disable_interface("eth0", TcGuiMessage::ResetUiState),
            ConfirmRequest::stop_scenario("eth0", TcGuiMessage::ResetUiState),
        ] {
            assert!(r.destructive, "{} should be a danger action", r.title);
            assert!(
                r.body.contains("eth0"),
                "{} does not name its target",
                r.title
            );
        }
    }
}
