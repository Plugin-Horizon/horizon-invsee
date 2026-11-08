fn invsee_tab_complete(game: &mut crate::game::Game<'_>, player: u128, command: &str, suggestions: &mut Vec<horizon_client::mod_api::Suggestion>) {
    crate::game::invsee_glue::tab_complete(game, player, command, suggestions);
}
