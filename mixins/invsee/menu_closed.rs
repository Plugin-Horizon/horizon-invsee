fn invsee_menu_closed(game: &mut crate::game::Game<'_>, uuid: u128, window: i32) {
    crate::game::invsee_glue::closed(game, uuid, window);
}
