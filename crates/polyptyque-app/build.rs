include!("../../packaging/ressources_windows.rs");

fn main() {
    // Thème sombre « Fluent » pour les contrôles standard (listes, champs…).
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("compilation de l'interface");
    ressources_windows("Polyptyque - enregistrement multi-fenêtres", "polyptyque");
}
