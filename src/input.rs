//! Input handling -- vim-style keyboard navigation.
//!
//! Dispatches madori `AppEvent::Key` events to the appropriate action
//! based on the current mode (Normal, Detail, Search, Command).

use crate::render::ViewMode;
use madori::event::KeyCode;

/// Actions that can be triggered by keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Navigate items down.
    Down,
    /// Navigate items up.
    Up,
    /// Select / open item detail.
    Select,
    /// Copy password for selected item.
    CopyPassword,
    /// Copy username for selected item.
    CopyUsername,
    /// Copy TOTP for selected item.
    CopyTotp,
    /// Enter search mode.
    EnterSearch,
    /// Toggle favorites filter.
    ToggleFavorites,
    /// Switch to next vault.
    NextVault,
    /// Go back (detail -> list, search -> list).
    Back,
    /// Quit the application.
    Quit,
    /// Toggle hidden field visibility (in detail mode).
    ToggleHidden,
    /// Copy the currently selected field (in detail mode).
    CopyField,
    /// Insert character into search input.
    SearchInput(char),
    /// Delete character from search input (backspace).
    SearchBackspace,
    /// Submit search query.
    SearchSubmit,
    /// No action.
    None,
}

/// Map a key event to an action based on the current view mode.
#[must_use]
pub fn map_key(
    key: &KeyCode,
    pressed: bool,
    modifiers: &madori::event::Modifiers,
    text: &Option<String>,
    mode: &ViewMode,
) -> Action {
    if !pressed {
        return Action::None;
    }

    match mode {
        ViewMode::VaultList | ViewMode::ItemList => map_normal(key, modifiers),
        ViewMode::ItemDetail => map_detail(key, modifiers),
        ViewMode::Search => map_search(key, text),
    }
}

fn map_normal(key: &KeyCode, _modifiers: &madori::event::Modifiers) -> Action {
    match key {
        KeyCode::Char('j') | KeyCode::Down => Action::Down,
        KeyCode::Char('k') | KeyCode::Up => Action::Up,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('p') => Action::CopyPassword,
        KeyCode::Char('u') => Action::CopyUsername,
        KeyCode::Char('t') => Action::CopyTotp,
        KeyCode::Char('/') => Action::EnterSearch,
        KeyCode::Char('f') => Action::ToggleFavorites,
        KeyCode::Tab => Action::NextVault,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Escape => Action::Back,
        _ => Action::None,
    }
}

fn map_detail(key: &KeyCode, _modifiers: &madori::event::Modifiers) -> Action {
    match key {
        KeyCode::Char('j') | KeyCode::Down => Action::Down,
        KeyCode::Char('k') | KeyCode::Up => Action::Up,
        KeyCode::Enter | KeyCode::Char('y') => Action::CopyField,
        KeyCode::Char('p') => Action::CopyPassword,
        KeyCode::Char('u') => Action::CopyUsername,
        KeyCode::Char('t') => Action::CopyTotp,
        KeyCode::Char('H') => Action::ToggleHidden,
        KeyCode::Char('q') | KeyCode::Escape => Action::Back,
        _ => Action::None,
    }
}

fn map_search(key: &KeyCode, text: &Option<String>) -> Action {
    match key {
        KeyCode::Escape => Action::Back,
        KeyCode::Enter => Action::SearchSubmit,
        KeyCode::Backspace => Action::SearchBackspace,
        KeyCode::Down => Action::Down,
        KeyCode::Up => Action::Up,
        _ => {
            // Try to get character from text field
            if let Some(t) = text {
                if let Some(c) = t.chars().next() {
                    return Action::SearchInput(c);
                }
            }
            // Fallback for Char variant
            if let KeyCode::Char(c) = key {
                Action::SearchInput(*c)
            } else {
                Action::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use madori::event::KeyCode;

    fn no_mods() -> madori::event::Modifiers {
        madori::event::Modifiers::default()
    }

    #[test]
    fn normal_mode_j_moves_down() {
        let action = map_key(&KeyCode::Char('j'), true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::Down);
    }

    #[test]
    fn normal_mode_k_moves_up() {
        let action = map_key(&KeyCode::Char('k'), true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::Up);
    }

    #[test]
    fn normal_mode_enter_selects() {
        let action = map_key(&KeyCode::Enter, true, &no_mods(), &None, &ViewMode::VaultList);
        assert_eq!(action, Action::Select);
    }

    #[test]
    fn normal_mode_p_copies_password() {
        let action = map_key(&KeyCode::Char('p'), true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::CopyPassword);
    }

    #[test]
    fn normal_mode_slash_enters_search() {
        let action = map_key(&KeyCode::Char('/'), true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::EnterSearch);
    }

    #[test]
    fn normal_mode_q_quits() {
        let action = map_key(&KeyCode::Char('q'), true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn detail_mode_escape_goes_back() {
        let action = map_key(&KeyCode::Escape, true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::Back);
    }

    #[test]
    fn detail_mode_h_toggles_hidden() {
        let action = map_key(&KeyCode::Char('H'), true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::ToggleHidden);
    }

    #[test]
    fn detail_mode_y_copies_field() {
        let action = map_key(&KeyCode::Char('y'), true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::CopyField);
    }

    #[test]
    fn detail_mode_p_copies_password() {
        let action = map_key(&KeyCode::Char('p'), true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::CopyPassword);
    }

    #[test]
    fn detail_mode_t_copies_totp() {
        let action = map_key(&KeyCode::Char('t'), true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::CopyTotp);
    }

    #[test]
    fn search_mode_escape_goes_back() {
        let action = map_key(&KeyCode::Escape, true, &no_mods(), &None, &ViewMode::Search);
        assert_eq!(action, Action::Back);
    }

    #[test]
    fn search_mode_text_input() {
        let action = map_key(
            &KeyCode::Char('a'),
            true,
            &no_mods(),
            &Some("a".into()),
            &ViewMode::Search,
        );
        assert_eq!(action, Action::SearchInput('a'));
    }

    #[test]
    fn search_mode_backspace() {
        let action = map_key(&KeyCode::Backspace, true, &no_mods(), &None, &ViewMode::Search);
        assert_eq!(action, Action::SearchBackspace);
    }

    #[test]
    fn search_mode_enter_submits() {
        let action = map_key(&KeyCode::Enter, true, &no_mods(), &None, &ViewMode::Search);
        assert_eq!(action, Action::SearchSubmit);
    }

    #[test]
    fn search_mode_arrow_navigates() {
        let down = map_key(&KeyCode::Down, true, &no_mods(), &None, &ViewMode::Search);
        assert_eq!(down, Action::Down);
        let up = map_key(&KeyCode::Up, true, &no_mods(), &None, &ViewMode::Search);
        assert_eq!(up, Action::Up);
    }

    #[test]
    fn key_release_is_noop() {
        let action = map_key(&KeyCode::Char('j'), false, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn tab_switches_vault() {
        let action = map_key(&KeyCode::Tab, true, &no_mods(), &None, &ViewMode::ItemList);
        assert_eq!(action, Action::NextVault);
    }

    #[test]
    fn detail_mode_enter_copies_field() {
        let action = map_key(&KeyCode::Enter, true, &no_mods(), &None, &ViewMode::ItemDetail);
        assert_eq!(action, Action::CopyField);
    }
}
