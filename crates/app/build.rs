fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Native look per target: Cupertino on macOS, Fluent everywhere else.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let style = if target_os == "macos" {
        "cupertino"
    } else {
        "fluent"
    };
    let mut config = slint_build::CompilerConfiguration::new()
        .with_style(style.to_owned())
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
