fn invsee_console_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    if !*ran && crate::game::invsee_glue::console(game, command) {
        *ran = true;
    }
}
