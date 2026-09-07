use parchmint_design_system::{
    PRODUCTION_ICON_NAMES, REQUIRED_SEMANTIC_ROLES, TOKENS, production_icon_svg, production_token,
};

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
fn production_tokens_has_complete_light_and_dark_semantic_roles() {
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
