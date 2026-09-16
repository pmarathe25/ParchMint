use parchmint_design_system::{
    PRODUCTION_ICON_NAMES, REQUIRED_SEMANTIC_ROLES, TOKENS, production_icon_svg, production_token,
};

#[test]
fn production_icons_are_source_authored_symbolic_vectors() {
    assert_eq!(
        PRODUCTION_ICON_NAMES.len(),
        PRODUCTION_ICON_NAMES
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );

    for name in PRODUCTION_ICON_NAMES {
        let icon = production_icon_svg(name).expect("production icon is registered");
        assert!(icon.contains("<svg"));
        assert!(icon.contains("viewBox=\"0 0"));
        assert!(icon.contains("fill=\"currentColor\"") || icon.contains("stroke=\"currentColor\""));
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
