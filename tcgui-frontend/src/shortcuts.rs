//! Keyboard shortcuts: one table that both dispatches and documents.
//!
//! The help overlay renders from the same [`SHORTCUTS`] slice the dispatcher
//! matches on, so a shortcut cannot exist without being documented and the
//! documentation cannot drift from the binding. Before this, the only record of
//! the bindings was a doc comment on the dispatcher.

use iced::keyboard::{Key, Modifiers};

use crate::messages::TcGuiMessage;
use crate::ui_state::AppTab;

/// What a shortcut does. Kept separate from [`TcGuiMessage`] because a few
/// entries (`Help`, `Dismiss`) resolve against application state in `update`
/// rather than mapping to one fixed message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Tab(AppTab),
    ToggleHelp,
    Dismiss,
}

/// One binding: how it is typed, how it is written down, and what it does.
pub struct Shortcut {
    /// Whether Ctrl must be held.
    pub ctrl: bool,
    /// Accepted `Key::Character` spellings (a key can have several, e.g. `+`
    /// and `=` on the same physical key).
    pub chars: &'static [&'static str],
    /// Accepted named key, for keys that are not characters.
    pub named: Option<iced::keyboard::key::Named>,
    /// How the combination is written in the help overlay.
    pub combo: &'static str,
    /// What the help overlay says it does.
    pub description: &'static str,
    /// What it does.
    pub action: Action,
}

use iced::keyboard::key::Named;

/// Every keyboard shortcut in the application.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        ctrl: true,
        chars: &["+", "="],
        named: None,
        combo: "Ctrl + +",
        description: "Zoom in",
        action: Action::ZoomIn,
    },
    Shortcut {
        ctrl: true,
        chars: &["-", "_", ")"],
        named: None,
        combo: "Ctrl + -",
        description: "Zoom out",
        action: Action::ZoomOut,
    },
    Shortcut {
        ctrl: true,
        chars: &["0"],
        named: None,
        combo: "Ctrl + 0",
        description: "Reset zoom to 100%",
        action: Action::ZoomReset,
    },
    Shortcut {
        ctrl: true,
        chars: &["1"],
        named: None,
        combo: "Ctrl + 1",
        description: "Switch to the Interfaces tab",
        action: Action::Tab(AppTab::Interfaces),
    },
    Shortcut {
        ctrl: true,
        chars: &["2"],
        named: None,
        combo: "Ctrl + 2",
        description: "Switch to the Scenarios tab",
        action: Action::Tab(AppTab::Scenarios),
    },
    // F1, not `?`. The event subscription is `event::listen()`, which is blind
    // to widget focus, so a bare `?` would open this overlay *and* type into
    // the interface-search box. F1 is never a text character.
    Shortcut {
        ctrl: false,
        chars: &[],
        named: Some(Named::F1),
        combo: "F1",
        description: "Show or hide this shortcut list",
        action: Action::ToggleHelp,
    },
    Shortcut {
        ctrl: true,
        chars: &["/"],
        named: None,
        combo: "Ctrl + /",
        description: "Show or hide this shortcut list",
        action: Action::ToggleHelp,
    },
    Shortcut {
        ctrl: false,
        chars: &[],
        named: Some(Named::Escape),
        combo: "Esc",
        description: "Close the topmost dialog or overlay",
        action: Action::Dismiss,
    },
];

/// Resolve a key press against [`SHORTCUTS`].
pub fn dispatch(key: &Key, modifiers: Modifiers) -> Option<Action> {
    SHORTCUTS
        .iter()
        .find(|s| {
            s.ctrl == modifiers.control()
                && match key {
                    Key::Character(c) => s.chars.contains(&c.as_str()),
                    Key::Named(n) => s.named == Some(*n),
                    _ => false,
                }
        })
        .map(|s| s.action)
}

/// The message a shortcut maps to, where that mapping is fixed. `ToggleHelp`
/// and `Dismiss` are resolved in `update` instead, because they depend on which
/// overlays are currently open.
pub fn action_message(action: Action) -> TcGuiMessage {
    match action {
        Action::ZoomIn => TcGuiMessage::ZoomIn,
        Action::ZoomOut => TcGuiMessage::ZoomOut,
        Action::ZoomReset => TcGuiMessage::ZoomReset,
        Action::Tab(tab) => TcGuiMessage::SwitchTab(tab),
        Action::ToggleHelp => TcGuiMessage::ToggleShortcutHelp,
        Action::Dismiss => TcGuiMessage::DismissTopOverlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every documented shortcut is actually reachable. This is the anti-drift
    /// guarantee: an entry that dispatch cannot resolve is a lie in the help
    /// overlay.
    #[test]
    fn every_shortcut_round_trips_through_dispatch() {
        for s in SHORTCUTS {
            let mods = if s.ctrl {
                Modifiers::CTRL
            } else {
                Modifiers::default()
            };
            let mut resolved = false;
            for c in s.chars {
                let key = Key::Character((*c).into());
                assert_eq!(
                    dispatch(&key, mods),
                    Some(s.action),
                    "char {c:?} of {:?} did not dispatch",
                    s.combo
                );
                resolved = true;
            }
            if let Some(n) = s.named {
                assert_eq!(
                    dispatch(&Key::Named(n), mods),
                    Some(s.action),
                    "named key of {:?} did not dispatch",
                    s.combo
                );
                resolved = true;
            }
            assert!(resolved, "{:?} binds no key at all", s.combo);
        }
    }

    #[test]
    fn every_shortcut_is_documented() {
        for s in SHORTCUTS {
            assert!(!s.combo.is_empty(), "a shortcut has no printable combo");
            assert!(
                !s.description.is_empty(),
                "{:?} has no description",
                s.combo
            );
        }
    }

    /// No two entries claim the same key press, which would make dispatch
    /// order-dependent and the help overlay ambiguous.
    #[test]
    fn no_two_shortcuts_claim_the_same_key() {
        let mut seen: Vec<(bool, String)> = Vec::new();
        for s in SHORTCUTS {
            for c in s.chars {
                let k = (s.ctrl, (*c).to_string());
                assert!(!seen.contains(&k), "duplicate binding for {k:?}");
                seen.push(k);
            }
            if let Some(n) = s.named {
                let k = (s.ctrl, format!("{n:?}"));
                assert!(!seen.contains(&k), "duplicate binding for {k:?}");
                seen.push(k);
            }
        }
    }

    /// Ctrl-less character shortcuts would fire while typing in the interface
    /// search box, because `event::listen()` is blind to widget focus.
    #[test]
    fn no_bare_character_shortcut_can_hijack_a_text_input() {
        for s in SHORTCUTS {
            if !s.ctrl {
                assert!(
                    s.chars.is_empty(),
                    "{:?} binds a bare character and would type into the search box",
                    s.combo
                );
            }
        }
    }
}
