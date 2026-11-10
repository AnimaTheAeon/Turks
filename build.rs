fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var_os("TURKS_CHECK_ONLY").is_none() {
        println!("cargo:rerun-if-changed=resources/turks.rc");
        embed_resource::compile("resources/turks.rc", embed_resource::NONE)
            .manifest_optional()
            .ok();
    }
}
