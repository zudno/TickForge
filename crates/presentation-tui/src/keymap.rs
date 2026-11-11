use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tickforge_domain::trading::OrderSide;

use crate::action::Action;
use crate::state::{AppState, Overlay};

/// Traduce un evento de terminal a una acción, según la ventana activa.
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

    match &state.overlay {
        Overlay::None => main_screen(key),
        Overlay::Help => matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
        )
        .then_some(Action::CloseOverlay),
        Overlay::Ticket(_) => ticket(key),
        // En las confirmaciones solo se acepta una respuesta explícita
        Overlay::Preview(_) => match key.code {
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Esc => Some(Action::CloseOverlay),
            _ => None,
        },
        Overlay::CancelConfirm(_) => match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('s') => Some(Action::Confirm),
            KeyCode::Esc | KeyCode::Char('n') => Some(Action::CloseOverlay),
            _ => None,
        },
        // El kill switch solo responde a una confirmación explícita
        Overlay::KillSwitch(_) => match key.code {
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Esc => Some(Action::CloseOverlay),
            _ => None,
        },
        // Al escribir un símbolo, todas las letras son texto (incluidas `q`, `b` o `s`)
        Overlay::AddSymbol(_) => match key.code {
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Esc => Some(Action::CloseOverlay),
            KeyCode::Backspace => Some(Action::AddSymbolBackspace),
            KeyCode::Char(character)
                if character.is_ascii_alphanumeric() || character == '.' || character == '-' =>
            {
                Some(Action::AddSymbolInput(character))
            }
            _ => None,
        },
    }
}

fn main_screen(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrevious),
        KeyCode::Tab => Some(Action::FocusNextPanel),
        KeyCode::Char('b') => Some(Action::OpenTicket(OrderSide::Buy)),
        KeyCode::Char('s') => Some(Action::OpenTicket(OrderSide::Sell)),
        KeyCode::Char('c') => Some(Action::RequestCancelSelected),
        KeyCode::Char('a') => Some(Action::OpenAddSymbol),
        KeyCode::Char('!') => Some(Action::OpenKillSwitch),
        KeyCode::Char('d') => Some(Action::RemoveSelectedSymbol),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Esc => Some(Action::CloseOverlay),
        _ => None,
    }
}

fn ticket(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Esc => Some(Action::CloseOverlay),
        KeyCode::Tab | KeyCode::Down => Some(Action::TicketNextField),
        KeyCode::BackTab | KeyCode::Up => Some(Action::TicketPreviousField),
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => Some(Action::TicketToggle),
        KeyCode::Backspace => Some(Action::TicketBackspace),
        KeyCode::Char(character) if character.is_ascii_digit() || character == '.' => {
            Some(Action::TicketInput(character))
        }
        _ => None,
    }
}

fn is_force_quit(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{OrderTicket, TradingMode};
    use chrono::DateTime;
    use crossterm::event::KeyEventState;
    use tickforge_domain::shared::Symbol;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn key(code: KeyCode, state: &AppState) -> Option<Action> {
        map_event(&press(code, KeyModifiers::NONE), state)
    }

    fn state() -> AppState {
        AppState::new(
            TradingMode::Paper,
            "Alpaca",
            Vec::new(),
            DateTime::UNIX_EPOCH,
        )
    }

    fn with_ticket() -> AppState {
        let mut state = state();
        state.overlay = Overlay::Ticket(OrderTicket::new(
            Symbol::new("AAPL").unwrap(),
            OrderSide::Buy,
            None,
            None,
        ));
        state
    }

    #[test]
    fn maps_main_screen_keys() {
        let state = state();
        assert_eq!(key(KeyCode::Char('j'), &state), Some(Action::SelectNext));
        assert_eq!(key(KeyCode::Up, &state), Some(Action::SelectPrevious));
        assert_eq!(key(KeyCode::Tab, &state), Some(Action::FocusNextPanel));
        assert_eq!(
            key(KeyCode::Char('b'), &state),
            Some(Action::OpenTicket(OrderSide::Buy))
        );
        assert_eq!(
            key(KeyCode::Char('s'), &state),
            Some(Action::OpenTicket(OrderSide::Sell))
        );
        assert_eq!(
            key(KeyCode::Char('c'), &state),
            Some(Action::RequestCancelSelected)
        );
        assert_eq!(key(KeyCode::Char('x'), &state), None);
    }

    #[test]
    fn ticket_captures_numbers_and_ignores_letters() {
        let state = with_ticket();
        assert_eq!(
            key(KeyCode::Char('7'), &state),
            Some(Action::TicketInput('7'))
        );
        assert_eq!(
            key(KeyCode::Char('.'), &state),
            Some(Action::TicketInput('.'))
        );
        // Letras como `q` o `b` no deben salir ni abrir otro ticket mientras se escribe
        assert_eq!(key(KeyCode::Char('q'), &state), None);
        assert_eq!(key(KeyCode::Char('b'), &state), None);
        assert_eq!(key(KeyCode::Enter, &state), Some(Action::Confirm));
        assert_eq!(
            key(KeyCode::BackTab, &state),
            Some(Action::TicketPreviousField)
        );
        assert_eq!(key(KeyCode::Right, &state), Some(Action::TicketToggle));
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
        assert_eq!(
            map_event(
                &press(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &with_ticket()
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn help_overlay_captures_keys() {
        let mut state = state();
        state.overlay = Overlay::Help;

        assert_eq!(key(KeyCode::Char('j'), &state), None);
        assert_eq!(key(KeyCode::Esc, &state), Some(Action::CloseOverlay));
    }
}
