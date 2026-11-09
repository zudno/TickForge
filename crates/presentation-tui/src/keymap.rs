use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::action::Action;
use crate::state::AppState;

/// Traduce un evento de terminal a una acción, según el contexto de la pantalla.
pub fn map_event(event: &Event, state: &AppState) -> Option<Action> {
    let Event::Key(key) = event else {
        return None;
    };
    // En Windows llegan eventos de presionar y soltar; solo interesa presionar
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if is_force_quit(key) {
        return Some(Action::Quit);
    }

    // Con la ayuda abierta, cualquier tecla de cierre la oculta y el resto se ignora
    if state.show_help {
        return matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
        )
        .then_some(Action::CloseOverlay);
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrevious),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Esc => Some(Action::CloseOverlay),
        _ => None,
    }
}

fn is_force_quit(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TradingMode;
    use chrono::DateTime;
    use crossterm::event::KeyEventState;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn state() -> AppState {
        AppState::new(
            TradingMode::Paper,
            "Alpaca",
            Vec::new(),
            DateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn maps_navigation_and_commands() {
        let state = state();
        let key = |code| map_event(&press(code, KeyModifiers::NONE), &state);

        assert_eq!(key(KeyCode::Char('j')), Some(Action::SelectNext));
        assert_eq!(key(KeyCode::Up), Some(Action::SelectPrevious));
        assert_eq!(key(KeyCode::Char('r')), Some(Action::Refresh));
        assert_eq!(key(KeyCode::Char('q')), Some(Action::Quit));
        assert_eq!(key(KeyCode::Char('x')), None);
    }

    #[test]
    fn ignores_key_releases() {
        let release = Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        assert_eq!(map_event(&release, &state()), None);
    }

    #[test]
    fn ctrl_c_always_quits() {
        let mut state = state();
        state.show_help = true;
        assert_eq!(
            map_event(&press(KeyCode::Char('c'), KeyModifiers::CONTROL), &state),
            Some(Action::Quit)
        );
    }

    #[test]
    fn help_overlay_captures_keys() {
        let mut state = state();
        state.show_help = true;

        assert_eq!(
            map_event(&press(KeyCode::Char('j'), KeyModifiers::NONE), &state),
            None
        );
        assert_eq!(
            map_event(&press(KeyCode::Esc, KeyModifiers::NONE), &state),
            Some(Action::CloseOverlay)
        );
    }
}
