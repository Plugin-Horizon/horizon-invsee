fn invsee_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &horizon_client::mod_api::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if !*ran
        && crate::game::invsee_glue::perform(game, player.uuid, &player.name, command)
    {
        *ran = true;
    }
}
