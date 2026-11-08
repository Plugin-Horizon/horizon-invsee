fn invsee_register_config(list: &mut Vec<crate::game::native_configs::NativeConfig>) {
    list.push(crate::game::native_configs::NativeConfig::new(
        "invsee",
        crate::game::native_configs::NativeConfigKind::Server,
        horizon_invsee::config::DEFAULT,
    ));
}
