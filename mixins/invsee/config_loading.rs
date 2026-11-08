fn invsee_config_loading(file: &crate::game::native_configs::NativeConfigFile) {
    if file.mod_id == "invsee" {
        crate::game::invsee_glue::config(&file.text, &file.path);
    }
}
