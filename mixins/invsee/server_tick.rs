// Once a tick, at its end (the hook fires at the start and at the end).
fn invsee_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    if matches!(phase, horizon_client::mod_api::ServerTickPhase::ServerTickEnd { .. }) {
        crate::game::invsee_glue::tick(game);
    }
}
