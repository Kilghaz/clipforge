fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Native look per target: Cupertino on macOS, Fluent everywhere else.
    // `CLIPFORGE_SLINT_STYLE=fluent` renders the Windows look on a Mac for
    // the visual pass (use a separate CARGO_TARGET_DIR, see CLAUDE.md).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let style = std::env::var("CLIPFORGE_SLINT_STYLE").unwrap_or_else(|_| {
        if target_os == "macos" {
            "cupertino".to_owned()
        } else {
            "fluent".to_owned()
        }
    });
    println!("cargo:rerun-if-env-changed=CLIPFORGE_SLINT_STYLE");
    let mut config = slint_build::CompilerConfiguration::new()
        .with_style(style)
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    // Element debug info lets the UI interaction tests find elements
    // (tests/ui_interaction.rs); debug builds only, releases stay lean.
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        config = config.with_debug_info(true);
    }
    slint_build::compile_with_config("ui/main.slint", config)?;
    println!("cargo:rerun-if-changed=lang");
    Ok(())
}
