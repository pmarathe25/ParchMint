use parchmint_design_system::{
    DesignSource, GenerationError, PRODUCTION_ICON_NAMES, generate,
    generated_penpot_tokens::{PENPOT_TOKEN_SOURCE_SHA256, REQUIRED_SEMANTIC_ROLES, TOKENS},
    production_icon_svg, production_token, validate_production_tokens,
};

const LIGHT_AND_DARK_TOKENS: &str = r##"
{
  "tokens": {
    "color.surface.application": {
      "type": "color",
      "light": "#f7f8f7",
      "dark": "#171a19"
    },
    "color.text.primary": {
      "type": "color",
      "light": "#202523",
      "dark": "#f0f3f1"
    },
    "font.family.ui": {
      "$type": "fontFamily",
      "$value": "sourcesanspro"
    },
    "font.family.legacy": {
      "type": "fontFamily",
      "value": "Inter"
    },
    "space.control.padding": {
      "type": "dimension",
      "value": "8px"
    }
  }
}
"##;

const ARROW_SVG: &str = r#"<svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg"><path d="M4 10h12m-5-5 5 5-5 5"/></svg>"#;
const ARROW_CHECKSUM: &str = "461642645702ad58db244fef7588226b3f7ce137963b889e0bcab3893db03c4c";

fn source(tokens: &str) -> DesignSource {
    DesignSource::from_token_json_and_icons(
        tokens,
        vec![("arrow-right".to_owned(), ARROW_SVG.to_owned())],
    )
}

#[test]
fn generates_deterministic_framework_neutral_themes_and_shared_vectors() {
    let design = generate(source(LIGHT_AND_DARK_TOKENS)).expect("valid design source");

    assert_eq!(design.token("space.control.padding").value(), "8px");
    assert_eq!(design.token("font.family.ui").value(), "Source Sans 3");
    assert_eq!(design.token("font.family.legacy").value(), "Source Sans 3");
    let icon = design.icon("arrow-right").expect("indexed icon");
    assert_eq!(icon.view_box(), "0 0 20 20");
    assert!(icon.is_monochrome());
    assert_eq!(icon.checksum(), ARROW_CHECKSUM);
    assert_eq!(design.icon_catalog().len(), 1);
    let light = design.theme_snapshot("Light", 1);
    let dark = design.theme_snapshot("Dark", 2);
    assert_eq!(light.role_names(), dark.role_names());
    assert_eq!(light.icon_catalog(), dark.icon_catalog());
    assert_ne!(
        light.token("color.surface.application"),
        dark.token("color.surface.application")
    );

    let aliased_light_value = LIGHT_AND_DARK_TOKENS.replace(
        "\"light\": \"#202523\"",
        "\"light\": \"{color.surface.application}\"",
    );
    let aliased = generate(source(&aliased_light_value)).expect("theme aliases should resolve");
    assert_eq!(
        aliased
            .theme_snapshot("Light", 1)
            .token("color.text.primary"),
        Some("#f7f8f7")
    );

    let repeated = generate(source(LIGHT_AND_DARK_TOKENS)).expect("same source remains valid");
    assert_eq!(design.generated_rust(), repeated.generated_rust());
    assert_eq!(design.source_digest(), repeated.source_digest());
    assert!(!design.generated_rust().contains("iced"));
    assert!(!design.generated_rust().contains("egui"));
    assert!(!design.generated_rust().contains("gtk"));
}

#[test]
fn production_icons_are_source_authored_symbolic_vectors() {
    assert_eq!(
        PRODUCTION_ICON_NAMES,
        [
            "launcher-project",
            "launcher-last-opened",
            "workspace-project",
            "workspace-editor",
            "workspace-cards",
            "workspace-history",
            "workspace-deleted",
            "workspace-export",
            "workspace-settings",
            "explorer-folder-closed",
            "explorer-folder-open",
            "format-bulleted-list",
            "format-block-quote",
            "format-link",
            "format-page-break",
        ]
    );

    for name in PRODUCTION_ICON_NAMES {
        let icon = production_icon_svg(name).expect("production icon is registered");
        assert!(icon.contains("<svg"));
        assert!(icon.contains("viewBox=\"0 0"));
        assert!(icon.contains("fill=\"currentColor\""));
        if !matches!(*name, "explorer-folder-closed" | "explorer-folder-open")
            && !name.starts_with("launcher-")
        {
            assert!(icon.contains("transform=\"translate(-"));
        }
    }
    assert!(production_icon_svg("unknown").is_none());
}

#[test]
fn rejects_invalid_token_names_values_aliases_and_theme_roles() {
    let duplicate = r##"{
      "tokens": [
        { "name": "color.surface.application", "type": "color", "light": "#111111", "dark": "#222222" },
        { "name": "color.surface.application", "type": "color", "light": "#333333", "dark": "#444444" },
        { "name": "color.text.primary", "type": "color", "light": "#111111", "dark": "#222222" }
      ]
    }"##;
    assert!(matches!(
        generate(source(duplicate)),
        Err(GenerationError::DuplicateToken { .. })
    ));

    let missing_dark = LIGHT_AND_DARK_TOKENS.replace("\"dark\": \"#171a19\"", "\"dark\": null");
    assert!(matches!(
        generate(source(&missing_dark)),
        Err(GenerationError::MissingThemeRole { .. })
    ));

    let missing_required_role = LIGHT_AND_DARK_TOKENS.replace(
        "\"color.text.primary\": {\n      \"type\": \"color\",\n      \"light\": \"#202523\",\n      \"dark\": \"#f0f3f1\"\n    },\n    ",
        "",
    );
    assert!(matches!(
        generate(source(&missing_required_role)),
        Err(GenerationError::MissingSemanticRole { .. })
    ));

    let missing_alias = LIGHT_AND_DARK_TOKENS.replace("\"8px\"", "\"{space.unknown}\"");
    assert!(matches!(
        generate(source(&missing_alias)),
        Err(GenerationError::MissingAlias { .. })
    ));

    let cycle = LIGHT_AND_DARK_TOKENS.replace("\"8px\"", "\"{space.control.alias}\"").replace(
        "\"space.control.padding\": {\n      \"type\": \"dimension\",\n      \"value\": \"{space.control.alias}\"\n    }",
        "\"space.control.padding\": {\n      \"type\": \"dimension\",\n      \"value\": \"{space.control.alias}\"\n    },\n    \"space.control.alias\": {\n      \"type\": \"dimension\",\n      \"value\": \"{space.control.padding}\"\n    }",
    );
    assert!(matches!(
        generate(source(&cycle)),
        Err(GenerationError::AliasCycle { .. })
    ));
}

#[test]
fn rejects_changed_checksums_and_invalid_vector_catalogs() {
    let changed_icon = source(LIGHT_AND_DARK_TOKENS).with_icon_checksum("arrow-right", "deadbeef");
    assert!(matches!(
        generate(changed_icon),
        Err(GenerationError::VectorChecksumMismatch { expected, .. }) if expected == "deadbeef"
    ));
    let changed_tokens = source(LIGHT_AND_DARK_TOKENS).with_token_checksum("deadbeef");
    assert!(matches!(
        generate(changed_tokens),
        Err(GenerationError::TokenChecksumMismatch { expected, .. }) if expected == "deadbeef"
    ));

    let no_icons = DesignSource::from_token_json_and_icons(LIGHT_AND_DARK_TOKENS, Vec::new());
    assert!(matches!(
        generate(no_icons),
        Err(GenerationError::MissingIcon { .. })
    ));

    let invalid_svg = DesignSource::from_token_json_and_icons(
        LIGHT_AND_DARK_TOKENS,
        vec![(
            "raster".to_owned(),
            "<svg viewBox=\"0 0 20 20\"><image/></svg>".to_owned(),
        )],
    );
    assert!(matches!(
        generate(invalid_svg),
        Err(GenerationError::InvalidSvg { .. })
    ));
}

#[test]
fn checked_in_penpot_snapshot_has_complete_light_and_dark_semantic_roles() {
    validate_production_tokens().expect("generated production tokens remain complete");
    assert_eq!(
        PENPOT_TOKEN_SOURCE_SHA256,
        "ad30015644a1d9c17c3bd8357d5e72c1e2e772a5adbd9288e8ff2b00a431ee70"
    );
    assert!(TOKENS.len() >= 50);
    for role in REQUIRED_SEMANTIC_ROLES {
        let token = production_token(role).expect("required role is emitted");
        assert!(!token.light.is_empty());
        assert!(!token.dark.is_empty());
    }
    assert_ne!(
        production_token("color.surface.manuscript").unwrap().light,
        production_token("color.surface.manuscript").unwrap().dark
    );
}
