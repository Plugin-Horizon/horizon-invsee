fn invsee_register_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    for (usage, level) in crate::game::invsee_glue::usages() {
        commands.push(horizon_client::mod_api::NativeCommand::op(&usage, level));
    }
}
