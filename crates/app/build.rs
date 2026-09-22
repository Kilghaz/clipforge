fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Native look per target: Cupertino on macOS, Fluent everywhere else.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let style = if target_os == "macos" {
        "cupertino"
    } else {
        "fluent"
    };
    let config = slint_build::CompilerConfiguration::new()
        .with_style(style.to_owned())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/main.slint", config)?;
    println!("cargo:rerun-if-changed=lang");
    Ok(())
}
