fn invsee_menu_clicked(game: &mut crate::game::Game<'_>, uuid: u128, window: i32, slot: i32, button: i32, mode: i32) {
    crate::game::invsee_glue::clicked(game, uuid, window, slot, button, mode);
}
