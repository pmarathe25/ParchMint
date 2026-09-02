//! Deterministic, framework-neutral data generated from ParchMint's UI source.

pub mod generated_penpot_tokens;

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use serde_json::Value;
use sha2::{Digest, Sha256};

const REQUIRED_SEMANTIC_ROLES: [&str; 2] = ["color.surface.application", "color.text.primary"];

/// Returns a checked-in production token record by semantic role.
pub fn production_token(name: &str) -> Option<&'static generated_penpot_tokens::GeneratedToken> {
    generated_penpot_tokens::TOKENS
        .iter()
        .find(|token| token.name == name)
}

/// Returns a checked-in, source-authored SVG by its semantic product name.
///
/// These vectors are framework-neutral and use `currentColor`, so renderers
/// select their color from the active semantic text role instead of carrying
/// appearance-specific assets.
pub fn production_icon_svg(name: &str) -> Option<&'static str> {
    match name {
        "launcher-project" => Some(LAUNCHER_PROJECT_ICON_SVG),
        "launcher-last-opened" => Some(LAUNCHER_LAST_OPENED_ICON_SVG),
        "workspace-project" => Some(WORKSPACE_PROJECT_ICON_SVG),
        "workspace-editor" => Some(WORKSPACE_EDITOR_ICON_SVG),
        "workspace-cards" => Some(WORKSPACE_CARDS_ICON_SVG),
        "workspace-history" => Some(WORKSPACE_HISTORY_ICON_SVG),
        "workspace-deleted" => Some(WORKSPACE_DELETED_ICON_SVG),
        "workspace-export" => Some(WORKSPACE_EXPORT_ICON_SVG),
        "workspace-settings" => Some(WORKSPACE_SETTINGS_ICON_SVG),
        "explorer-folder-closed" => Some(EXPLORER_FOLDER_CLOSED_ICON_SVG),
        "explorer-folder-open" => Some(EXPLORER_FOLDER_OPEN_ICON_SVG),
        "format-bulleted-list" => Some(FORMAT_BULLETED_LIST_ICON_SVG),
        "format-block-quote" => Some(FORMAT_BLOCK_QUOTE_ICON_SVG),
        "format-link" => Some(FORMAT_LINK_ICON_SVG),
        "format-page-break" => Some(FORMAT_PAGE_BREAK_ICON_SVG),
        _ => None,
    }
}

/// Semantic names for the checked-in product vector catalog.
pub const PRODUCTION_ICON_NAMES: &[&str] = &[
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
];

/// The 20 px project-folder vector used by launcher recent-project cards.
///
/// The path is the checked-in Penpot component geometry, normalized to its
/// 20 px component bounds.
pub const LAUNCHER_PROJECT_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" d="M8.333496 3.333496H3.333496C2.416504 3.333496 1.666504 4.083496 1.666504 5V15C1.666504 15.916504 2.416504 16.666504 3.333496 16.666504H16.666504C17.583496 16.666504 18.333496 15.916504 18.333496 15V6.666504C18.333496 5.75 17.583496 5 16.666504 5H10L8.333496 3.333496ZM16.666504 15H3.333496V5H7.641602L9.308105 6.666504H16.666504V15Z"/></svg>"#;

/// The 14 px last-opened clock vector used by launcher recent-project cards.
///
/// The path is the checked-in Penpot component geometry, normalized to its
/// 14 px component bounds.
pub const LAUNCHER_LAST_OPENED_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14"><path fill="currentColor" d="M6.994141 1.166504C3.773438 1.169922 1.164551 3.783203 1.166504 7.003906C1.168945 10.224609 3.78125 12.834473 7.001953 12.833496C10.222656 12.832031 12.833496 10.220703 12.833496 7C12.833496 5.45166 12.217773 3.967285 11.122559 2.873047C10.027344 1.778809 8.54248 1.165039 6.994141 1.166504ZM7 11.666504C4.422852 11.666504 2.333496 9.577148 2.333496 7C2.333496 4.422852 4.422852 2.333496 7 2.333496C9.577148 2.333496 11.666504 4.422852 11.666504 7C11.666504 9.577148 9.577148 11.666504 7 11.666504ZM7.291504 4.083496H6.416504V7.583496L9.479004 9.420898L9.916504 8.703125L7.291504 7.145996Z"/></svg>"#;

/// The 20 px Project vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_PROJECT_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7492 -303)" d="M7497.0,304.6666564941406L7503.66650390625,304.6666564941406L7507.0,308.0L7507.0,321.3333435058594L7497.0,321.3333435058594L7497.0,304.6666564941406ZM7502.83349609375,306.3333435058594L7498.66650390625,306.3333435058594L7498.66650390625,319.6666564941406L7505.33349609375,319.6666564941406L7505.33349609375,308.8333435058594L7502.83349609375,308.8333435058594L7502.83349609375,306.3333435058594ZM7500.33349609375,312.1666564941406L7503.66650390625,312.1666564941406L7503.66650390625,313.8333435058594L7500.33349609375,313.8333435058594L7500.33349609375,312.1666564941406ZM7500.33349609375,315.5L7503.66650390625,315.5L7503.66650390625,317.1666564941406L7500.33349609375,317.1666564941406L7500.33349609375,315.5Z"/></svg>"#;

/// The 20 px Editor vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_EDITOR_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7723 -303)" d="M7725.5,317.375L7725.5,320.5L7728.625,320.5L7737.841796875,311.2833251953125L7734.716796875,308.1583251953125L7725.5,317.375ZM7740.25830078125,308.8666687011719C7740.58154296875,308.5416259765625,7740.58154296875,308.0166931152344,7740.25830078125,307.6916809082031L7738.30810546875,305.7416687011719C7737.9833984375,305.4185791015625,7737.45849609375,305.4185791015625,7737.13330078125,305.7416687011719L7735.6083984375,307.26666259765625L7738.7333984375,310.39166259765625L7740.25830078125,308.8666687011719Z"/></svg>"#;

/// The 20 px Cards vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_CARDS_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7765 -303)" d="M7767.5,305.5L7774.16650390625,305.5L7774.16650390625,312.1666564941406L7767.5,312.1666564941406L7767.5,305.5M7775.83349609375,305.5L7782.5,305.5L7782.5,312.1666564941406L7775.83349609375,312.1666564941406L7775.83349609375,305.5M7767.5,313.8333435058594L7774.16650390625,313.8333435058594L7774.16650390625,320.5L7767.5,320.5L7767.5,313.8333435058594M7775.83349609375,313.8333435058594L7782.5,313.8333435058594L7782.5,320.5L7775.83349609375,320.5L7775.83349609375,313.8333435058594Z"/></svg>"#;

/// The 20 px History vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_HISTORY_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7807 -303)" d="M7817.83349609375,305.5C7814.01171875,305.49755859375,7810.7998046875,308.36883544921875,7810.375,312.1666564941406L7807.83349609375,312.1666564941406L7811.16650390625,315.5L7814.5,312.1666564941406L7812.0498046875,312.1666564941406C7812.39892578125,309.4822998046875,7814.548828125,307.3907470703125,7817.24169921875,307.11566162109375C7819.9345703125,306.8405456542969,7822.46337890625,308.4542236328125,7823.34765625,311.0125732421875C7824.232421875,313.5709228515625,7823.24072265625,316.40167236328125,7820.953125,317.84869384765625C7818.6650390625,319.2957458496094,7815.6826171875,318.97882080078125,7813.75,317.0833435058594L7812.56689453125,318.26666259765625C7814.96142578125,320.6942138671875,7818.6923828125,321.2008361816406,7821.6474609375,319.499755859375C7824.6025390625,317.7987060546875,7826.03857421875,314.31817626953125,7825.14208984375,311.0281982421875C7824.24609375,307.73822021484375,7821.2431640625,305.4669494628906,7817.83349609375,305.5ZM7817.0,309.6666564941406L7817.0,313.8333435058594L7820.54150390625,315.9333190917969L7821.16650390625,314.8666687011719L7818.25,313.1333312988281L7818.25,309.6666564941406L7817.0,309.6666564941406Z"/></svg>"#;

/// The 20 px Recently Deleted vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_DELETED_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7849 -303)" d="M7854.0,318.8333435058594C7854.0,319.75,7854.75,320.5,7855.66650390625,320.5L7862.33349609375,320.5C7863.25,320.5,7864.0,319.75,7864.0,318.8333435058594L7864.0,308.8333435058594L7854.0,308.8333435058594L7854.0,318.8333435058594ZM7855.66650390625,310.5L7862.33349609375,310.5L7862.33349609375,318.8333435058594L7855.66650390625,318.8333435058594L7855.66650390625,310.5ZM7861.91650390625,306.3333435058594L7861.08349609375,305.5L7856.91650390625,305.5L7856.08349609375,306.3333435058594L7853.16650390625,306.3333435058594L7853.16650390625,308.0L7864.83349609375,308.0L7864.83349609375,306.3333435058594Z"/></svg>"#;

/// The 20 px Export vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_EXPORT_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7891 -303)" d="M7900.16650390625,305.5L7901.83349609375,305.5L7901.83349609375,313.8333435058594L7904.75,310.9166564941406L7905.93310546875,312.1000061035156L7901.0,317.0333251953125L7896.06689453125,312.1000061035156L7897.25,310.9166564941406L7900.16650390625,313.8333435058594L7900.16650390625,305.5ZM7894.33349609375,318.8333435058594L7907.66650390625,318.8333435058594L7907.66650390625,320.5L7894.33349609375,320.5L7894.33349609375,318.8333435058594Z"/></svg>"#;

/// The 20 px Settings vector from Penpot's `WorkspaceTopBar` component.
pub const WORKSPACE_SETTINGS_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-7933 -303)" d="M7949.19189453125,313.8166809082031C7949.22509765625,313.54998779296875,7949.25,313.2749938964844,7949.25,313.0C7949.25,312.7250061035156,7949.22509765625,312.45001220703125,7949.18310546875,312.1833190917969L7950.94189453125,310.8083190917969L7949.27490234375,307.92498779296875L7947.2001953125,308.7583312988281C7946.7685546875,308.4250793457031,7946.294921875,308.15045166015625,7945.79150390625,307.9416809082031L7945.5,305.7083435058594L7942.16650390625,305.7083435058594L7941.85009765625,307.9416809082031C7941.341796875,308.1499938964844,7940.875,308.42498779296875,7940.44189453125,308.7583312988281L7938.36669921875,307.92498779296875L7936.7001953125,310.8083190917969L7938.45849609375,312.1833190917969C7938.4248046875,312.45001220703125,7938.3916015625,312.73333740234375,7938.3916015625,313.0C7938.3916015625,313.26666259765625,7938.41650390625,313.54998779296875,7938.45849609375,313.8166809082031L7936.7001953125,315.1916809082031L7938.36669921875,318.07501220703125L7940.44189453125,317.2416687011719C7940.875,317.57501220703125,7941.341796875,317.8500061035156,7941.85009765625,318.0583190917969L7942.16650390625,320.2916564941406L7945.5,320.2916564941406L7945.81689453125,318.0583190917969C7946.3251953125,317.8500061035156,7946.79150390625,317.57501220703125,7947.22509765625,317.2416687011719L7949.2998046875,318.07501220703125L7950.966796875,315.1916809082031L7949.19189453125,313.8166809082031ZM7943.83349609375,315.9166564941406C7942.107421875,315.9166564941406,7940.70849609375,314.5175476074219,7940.70849609375,312.7916564941406C7940.70849609375,311.0657653808594,7942.107421875,309.6666564941406,7943.83349609375,309.6666564941406C7945.55908203125,309.6666564941406,7946.95849609375,311.0657653808594,7946.95849609375,312.7916564941406C7946.95849609375,314.5175476074219,7945.55908203125,315.9166564941406,7943.83349609375,315.9166564941406Z"/></svg>"#;

/// Compact folder states used by the Explorer hierarchy.
pub const EXPLORER_FOLDER_CLOSED_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" d="M2.5 5.25c0-.97.78-1.75 1.75-1.75h4l1.75 2h5.75c.97 0 1.75.78 1.75 1.75v7.5c0 .97-.78 1.75-1.75 1.75H4.25c-.97 0-1.75-.78-1.75-1.75v-9.5Zm1.75.25v9h11.5V7.25H9.18L7.43 5.5H4.25Z"/></svg>"#;
pub const EXPLORER_FOLDER_OPEN_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" d="M2.5 5.25c0-.97.78-1.75 1.75-1.75h4l1.75 2h5.75c.97 0 1.75.78 1.75 1.75v1H6.04l-1.75 6h11.46l1.75-6H10L8.25 5.5h-4v9h.04L2.54 8.5H2.5V5.25Zm2.88 4.75h10.87l-1.17 4H4.21l1.17-4Z"/></svg>"#;

/// The 18 px bulleted-list vector from Penpot's `FormattingToolbar` component.
pub const FORMAT_BULLETED_LIST_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><path fill="currentColor" transform="translate(-3302 -1482)" d="M3305.0,1486.5L3306.5,1486.5L3306.5,1488.0L3305.0,1488.0L3305.0,1486.5ZM3308.0,1486.5L3317.0,1486.5L3317.0,1488.0L3308.0,1488.0L3308.0,1486.5ZM3305.0,1490.25L3306.5,1490.25L3306.5,1491.75L3305.0,1491.75L3305.0,1490.25ZM3308.0,1490.25L3317.0,1490.25L3317.0,1491.75L3308.0,1491.75L3308.0,1490.25ZM3305.0,1494.0L3306.5,1494.0L3306.5,1495.5L3305.0,1495.5L3305.0,1494.0ZM3308.0,1494.0L3317.0,1494.0L3317.0,1495.5L3308.0,1495.5L3308.0,1494.0Z"/></svg>"#;

/// The 20 px block-quote vector from Penpot's `FormattingToolbar` component.
pub const FORMAT_BLOCK_QUOTE_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-3358 -1481)" d="M3363.833251953125,1495.1666259765625L3367.166748046875,1495.1666259765625L3368.833251953125,1491.8333740234375L3368.833251953125,1486.8333740234375L3363.0,1486.8333740234375L3363.0,1491.8333740234375L3366.333251953125,1491.8333740234375L3363.833251953125,1495.1666259765625ZM3370.5,1495.1666259765625L3373.833251953125,1495.1666259765625L3375.5,1491.8333740234375L3375.5,1486.8333740234375L3369.666748046875,1486.8333740234375L3369.666748046875,1491.8333740234375L3373.0,1491.8333740234375L3370.5,1495.1666259765625Z"/></svg>"#;

/// The 20 px link vector from Penpot's `FormattingToolbar` component.
pub const FORMAT_LINK_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-3398 -1481)" d="M3406.824951171875,1492.175048828125C3407.47509765625,1492.8211669921875,3408.52490234375,1492.8211669921875,3409.175048828125,1492.175048828125L3411.675048828125,1489.675048828125C3412.150634765625,1489.2677001953125,3412.35791015625,1488.628173828125,3412.21142578125,1488.019287109375C3412.06494140625,1487.4105224609375,3411.589599609375,1486.93505859375,3410.980712890625,1486.7886962890625C3410.371826171875,1486.6422119140625,3409.732421875,1486.849365234375,3409.324951171875,1487.324951171875L3408.35009765625,1488.300048828125L3407.166748046875,1487.11669921875L3408.1416015625,1486.1417236328125C3409.444091796875,1484.8392333984375,3411.555908203125,1484.8392333984375,3412.8583984375,1486.1417236328125C3414.160888671875,1487.444091796875,3414.160888671875,1489.555908203125,3412.8583984375,1490.8582763671875L3410.3583984375,1493.3582763671875C3409.733154296875,1493.9842529296875,3408.884765625,1494.3359375,3408.0,1494.3359375C3407.115234375,1494.3359375,3406.266845703125,1493.9842529296875,3405.6416015625,1493.3582763671875L3406.824951171875,1492.175048828125ZM3409.175048828125,1489.824951171875C3408.52490234375,1489.1788330078125,3407.47509765625,1489.1788330078125,3406.824951171875,1489.824951171875L3404.324951171875,1492.324951171875C3403.849365234375,1492.7322998046875,3403.64208984375,1493.371826171875,3403.78857421875,1493.980712890625C3403.93505859375,1494.5894775390625,3404.410400390625,1495.06494140625,3405.019287109375,1495.2113037109375C3405.628173828125,1495.3577880859375,3406.267578125,1495.150634765625,3406.675048828125,1494.675048828125L3407.64990234375,1493.699951171875L3408.833251953125,1494.88330078125L3407.8583984375,1495.8582763671875C3406.555908203125,1497.1607666015625,3404.444091796875,1497.1607666015625,3403.1416015625,1495.8582763671875C3401.839111328125,1494.555908203125,3401.839111328125,1492.444091796875,3403.1416015625,1491.1417236328125L3405.6416015625,1488.6417236328125C3406.266845703125,1488.0157470703125,3407.115234375,1487.6640625,3408.0,1487.6640625C3408.884765625,1487.6640625,3409.733154296875,1488.0157470703125,3410.3583984375,1488.6417236328125L3409.175048828125,1489.824951171875Z"/></svg>"#;

/// The page-break vector used by the editor formatting toolbar. Its folded
/// page outline and dashed horizontal rule match common word-processor
/// page-break controls without depending on text glyph availability.
pub const FORMAT_PAGE_BREAK_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path fill="currentColor" transform="translate(-3420 -1480)" fill-rule="evenodd" d="M3423,1481H3432L3437,1486V1499H3423V1481ZM3424.5,1482.5V1497.5H3435.5V1486.75L3431.25,1482.5H3424.5ZM3433,1483.5V1486H3435.5L3433,1483.5ZM3425.5,1490H3428.5V1491.5H3425.5V1490ZM3430,1490H3432.5V1491.5H3430V1490ZM3434,1490H3435.5V1491.5H3434V1490Z"/></svg>"#;

/// Checks that the checked-in generated data remains a complete two-appearance
/// semantic foundation. The source archive is intentionally not opened here.
pub fn validate_production_tokens() -> Result<(), GenerationError> {
    for role in generated_penpot_tokens::REQUIRED_SEMANTIC_ROLES {
        let Some(token) = production_token(role) else {
            return Err(GenerationError::MissingSemanticRole {
                role: (*role).to_owned(),
            });
        };
        if token.light.is_empty() || token.dark.is_empty() {
            return Err(GenerationError::MissingThemeRole {
                role: (*role).to_owned(),
                appearance: if token.light.is_empty() {
                    "Light"
                } else {
                    "Dark"
                }
                .to_owned(),
            });
        }
    }
    Ok(())
}

/// The source material used to generate a design-system snapshot.
#[derive(Clone, Debug)]
pub struct DesignSource {
    token_json: String,
    token_checksum: String,
    icons: Vec<SourceIcon>,
}

#[derive(Clone, Debug)]
struct SourceIcon {
    name: String,
    svg: String,
    checksum: String,
}

impl DesignSource {
    /// Builds a source from a DTCG-style token document and product SVG vectors.
    pub fn from_token_json_and_icons(
        token_json: impl Into<String>,
        icons: Vec<(String, String)>,
    ) -> Self {
        let token_json = token_json.into();
        Self {
            token_checksum: sha256(token_json.as_bytes()),
            token_json,
            icons: icons
                .into_iter()
                .map(|(name, svg)| SourceIcon {
                    checksum: sha256(svg.as_bytes()),
                    name,
                    svg,
                })
                .collect(),
        }
    }

    /// Replaces the recorded token-source checksum. This is useful when reading
    /// an externally indexed design export.
    #[must_use]
    pub fn with_token_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.token_checksum = checksum.into();
        self
    }

    /// Replaces an icon's recorded source checksum.
    #[must_use]
    pub fn with_icon_checksum(mut self, name: &str, checksum: impl Into<String>) -> Self {
        if let Some(icon) = self.icons.iter_mut().find(|icon| icon.name == name) {
            icon.checksum = checksum.into();
        }
        self
    }
}

/// Errors found while compiling the maintained design source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationError {
    InvalidTokenSource {
        message: String,
    },
    InvalidToken {
        name: String,
        message: String,
    },
    TokenChecksumMismatch {
        expected: String,
        actual: String,
    },
    DuplicateToken {
        name: String,
    },
    MissingAlias {
        token: String,
        alias: String,
    },
    AliasCycle {
        token: String,
    },
    MissingThemeRole {
        role: String,
        appearance: String,
    },
    MissingSemanticRole {
        role: String,
    },
    MissingIcon {
        icon: String,
    },
    DuplicateIcon {
        icon: String,
    },
    VectorChecksumMismatch {
        icon: String,
        expected: String,
        actual: String,
    },
    InvalidSvg {
        icon: String,
        message: String,
    },
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTokenSource { message } => {
                write!(formatter, "invalid token source: {message}")
            }
            Self::InvalidToken { name, message } => {
                write!(formatter, "invalid token {name}: {message}")
            }
            Self::TokenChecksumMismatch { expected, actual } => write!(
                formatter,
                "token source checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::DuplicateToken { name } => write!(formatter, "duplicate token: {name}"),
            Self::MissingAlias { token, alias } => {
                write!(formatter, "token {token} aliases missing token {alias}")
            }
            Self::AliasCycle { token } => write!(formatter, "token alias cycle includes {token}"),
            Self::MissingThemeRole { role, appearance } => {
                write!(formatter, "token {role} has no {appearance} value")
            }
            Self::MissingSemanticRole { role } => {
                write!(formatter, "missing semantic role: {role}")
            }
            Self::MissingIcon { icon } => write!(formatter, "missing product icon: {icon}"),
            Self::DuplicateIcon { icon } => write!(formatter, "duplicate icon: {icon}"),
            Self::VectorChecksumMismatch {
                icon,
                expected,
                actual,
            } => write!(
                formatter,
                "vector checksum mismatch for {icon}: expected {expected}, got {actual}"
            ),
            Self::InvalidSvg { icon, message } => {
                write!(formatter, "invalid SVG {icon}: {message}")
            }
        }
    }
}

impl std::error::Error for GenerationError {}

/// A generated semantic token. Values are strings because the design-token
/// source can represent colors, dimensions, and font names without choosing a
/// UI toolkit type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticToken {
    name: String,
    token_type: String,
    value: String,
    light: Option<String>,
    dark: Option<String>,
}

impl SemanticToken {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn token_type(&self) -> &str {
        &self.token_type
    }

    /// Returns the shared value, or the Light value for a themed token.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A source SVG kept as vector geometry and colored by semantic roles at render time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorIcon {
    name: String,
    view_box: String,
    checksum: String,
}

impl VectorIcon {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn view_box(&self) -> &str {
        &self.view_box
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub fn is_monochrome(&self) -> bool {
        true
    }
}

/// The shared icon catalog for both appearances.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IconCatalog {
    icons: BTreeMap<String, VectorIcon>,
}

impl IconCatalog {
    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&VectorIcon> {
        self.icons.get(name)
    }
}

/// Framework-neutral result of compiling a design source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedDesignSystem {
    tokens: BTreeMap<String, SemanticToken>,
    icons: IconCatalog,
    source_digest: String,
    generated_rust: String,
}

impl GeneratedDesignSystem {
    pub fn token(&self, name: &str) -> &SemanticToken {
        &self.tokens[name]
    }

    pub fn icon(&self, name: &str) -> Option<&VectorIcon> {
        self.icons.get(name)
    }

    pub fn icon_catalog(&self) -> &IconCatalog {
        &self.icons
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    /// Generated Rust data with a stable ordering and no framework references.
    pub fn generated_rust(&self) -> &str {
        &self.generated_rust
    }

    pub fn theme_snapshot(&self, appearance: &str, generation: u64) -> ThemeSnapshot {
        let use_dark = appearance.eq_ignore_ascii_case("dark");
        let tokens = self
            .tokens
            .iter()
            .map(|(name, token)| {
                let value = if use_dark {
                    token.dark.as_ref().unwrap_or(&token.value)
                } else {
                    token.light.as_ref().unwrap_or(&token.value)
                };
                (name.clone(), value.clone())
            })
            .collect();

        ThemeSnapshot {
            appearance: if use_dark { "Dark" } else { "Light" }.to_owned(),
            generation,
            tokens,
            icons: self.icons.clone(),
        }
    }
}

/// The selected values and shared vectors for one appearance generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeSnapshot {
    appearance: String,
    generation: u64,
    tokens: BTreeMap<String, String>,
    icons: IconCatalog,
}

impl ThemeSnapshot {
    pub fn appearance(&self) -> &str {
        &self.appearance
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn token(&self, role: &str) -> Option<&str> {
        self.tokens.get(role).map(String::as_str)
    }

    pub fn role_names(&self) -> Vec<&str> {
        self.tokens.keys().map(String::as_str).collect()
    }

    pub fn icon_catalog(&self) -> &IconCatalog {
        &self.icons
    }
}

#[derive(Clone, Debug)]
struct RawToken {
    token_type: String,
    shared: Option<String>,
    light: Option<String>,
    dark: Option<String>,
}

impl RawToken {
    fn is_themed(&self) -> bool {
        self.light.is_some() || self.dark.is_some()
    }
}

/// Parses, validates, resolves, and deterministically renders the design source.
pub fn generate(source: DesignSource) -> Result<GeneratedDesignSystem, GenerationError> {
    verify_checksum(&source.token_checksum, source.token_json.as_bytes()).map_err(
        |(expected, actual)| GenerationError::TokenChecksumMismatch { expected, actual },
    )?;

    let raw_tokens = parse_tokens(&source.token_json)?;
    require_semantic_roles(&raw_tokens)?;
    let tokens = resolve_tokens(&raw_tokens)?;
    let icons = parse_icons(source.icons)?;
    let source_digest = source_digest(&source.token_json, &icons);
    let generated_rust = render_rust(&tokens, &icons, &source_digest);

    Ok(GeneratedDesignSystem {
        tokens,
        icons,
        source_digest,
        generated_rust,
    })
}

fn parse_tokens(token_json: &str) -> Result<BTreeMap<String, RawToken>, GenerationError> {
    let document: Value =
        serde_json::from_str(token_json).map_err(|error| GenerationError::InvalidTokenSource {
            message: error.to_string(),
        })?;
    let tokens = document
        .get("tokens")
        .ok_or_else(|| GenerationError::InvalidTokenSource {
            message: "missing tokens property".to_owned(),
        })?;

    let mut result = BTreeMap::new();
    match tokens {
        Value::Object(entries) => {
            for (name, definition) in entries {
                insert_token(&mut result, name, definition)?;
            }
        }
        Value::Array(entries) => {
            for definition in entries {
                let object =
                    definition
                        .as_object()
                        .ok_or_else(|| GenerationError::InvalidTokenSource {
                            message: "token array entries must be objects".to_owned(),
                        })?;
                let name = object.get("name").and_then(Value::as_str).ok_or_else(|| {
                    GenerationError::InvalidTokenSource {
                        message: "token array entry is missing a string name".to_owned(),
                    }
                })?;
                insert_token(&mut result, name, definition)?;
            }
        }
        _ => {
            return Err(GenerationError::InvalidTokenSource {
                message: "tokens must be an object or array".to_owned(),
            });
        }
    }
    Ok(result)
}

fn insert_token(
    result: &mut BTreeMap<String, RawToken>,
    name: &str,
    definition: &Value,
) -> Result<(), GenerationError> {
    if name.is_empty() {
        return Err(GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "name cannot be empty".to_owned(),
        });
    }
    if result.contains_key(name) {
        return Err(GenerationError::DuplicateToken {
            name: name.to_owned(),
        });
    }
    let object = definition
        .as_object()
        .ok_or_else(|| GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "definition must be an object".to_owned(),
        })?;
    let token_type = object
        .get("type")
        .or_else(|| object.get("$type"))
        .and_then(Value::as_str)
        .ok_or_else(|| GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "missing string type".to_owned(),
        })?
        .to_owned();
    let has_light = object.contains_key("light");
    let has_dark = object.contains_key("dark");
    let light = scalar_value(object.get("light"));
    let dark = scalar_value(object.get("dark"));
    if has_light || has_dark {
        if light.is_none() {
            return Err(GenerationError::MissingThemeRole {
                role: name.to_owned(),
                appearance: "Light".to_owned(),
            });
        }
        if dark.is_none() {
            return Err(GenerationError::MissingThemeRole {
                role: name.to_owned(),
                appearance: "Dark".to_owned(),
            });
        }
    }
    let shared = scalar_value(object.get("value").or_else(|| object.get("$value")));
    if shared.is_none() && light.is_none() {
        return Err(GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "missing value or Light/Dark values".to_owned(),
        });
    }
    result.insert(
        name.to_owned(),
        RawToken {
            token_type,
            shared,
            light,
            dark,
        },
    );
    Ok(())
}

fn scalar_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn require_semantic_roles(tokens: &BTreeMap<String, RawToken>) -> Result<(), GenerationError> {
    for role in REQUIRED_SEMANTIC_ROLES {
        if !tokens.contains_key(role) {
            return Err(GenerationError::MissingSemanticRole {
                role: role.to_owned(),
            });
        }
    }
    Ok(())
}

fn resolve_tokens(
    raw_tokens: &BTreeMap<String, RawToken>,
) -> Result<BTreeMap<String, SemanticToken>, GenerationError> {
    let mut resolved = BTreeMap::new();
    for (name, raw) in raw_tokens {
        let light = raw
            .is_themed()
            .then(|| resolve_value(raw_tokens, name, Some("Light"), &mut Vec::new()))
            .transpose()?;
        let dark = raw
            .is_themed()
            .then(|| resolve_value(raw_tokens, name, Some("Dark"), &mut Vec::new()))
            .transpose()?;
        let shared = if raw.is_themed() {
            None
        } else {
            Some(resolve_value(raw_tokens, name, None, &mut Vec::new())?)
        };
        let value = light.clone().or_else(|| shared.clone()).ok_or_else(|| {
            GenerationError::InvalidToken {
                name: name.clone(),
                message: "could not resolve a value".to_owned(),
            }
        })?;
        resolved.insert(
            name.clone(),
            SemanticToken {
                name: name.clone(),
                token_type: raw.token_type.clone(),
                value,
                light,
                dark,
            },
        );
    }
    Ok(resolved)
}

fn resolve_value(
    tokens: &BTreeMap<String, RawToken>,
    name: &str,
    appearance: Option<&str>,
    visiting: &mut Vec<String>,
) -> Result<String, GenerationError> {
    if visiting.iter().any(|token| token == name) {
        return Err(GenerationError::AliasCycle {
            token: name.to_owned(),
        });
    }
    visiting.push(name.to_owned());
    let result = (|| {
        let token = tokens
            .get(name)
            .ok_or_else(|| GenerationError::MissingAlias {
                token: name.to_owned(),
                alias: name.to_owned(),
            })?;
        let value = match appearance {
            Some("Light") => token.light.as_ref().or(token.shared.as_ref()),
            Some("Dark") => token.dark.as_ref().or(token.shared.as_ref()),
            _ => token.shared.as_ref(),
        }
        .ok_or_else(|| GenerationError::MissingThemeRole {
            role: name.to_owned(),
            appearance: appearance.unwrap_or("shared").to_owned(),
        })?;
        if let Some(alias) = alias_target(value) {
            if !tokens.contains_key(alias) {
                return Err(GenerationError::MissingAlias {
                    token: name.to_owned(),
                    alias: alias.to_owned(),
                });
            }
            resolve_value(tokens, alias, appearance, visiting)
        } else {
            Ok(normalize_value(&token.token_type, value))
        }
    })();
    visiting.pop();
    result
}

fn alias_target(value: &str) -> Option<&str> {
    value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .filter(|value| !value.is_empty())
}

fn normalize_value(token_type: &str, value: &str) -> String {
    if token_type == "fontFamily"
        && (value.eq_ignore_ascii_case("sourcesanspro") || value.eq_ignore_ascii_case("inter"))
    {
        "Source Sans 3".to_owned()
    } else {
        value.to_owned()
    }
}

fn parse_icons(source_icons: Vec<SourceIcon>) -> Result<IconCatalog, GenerationError> {
    if source_icons.is_empty() {
        return Err(GenerationError::MissingIcon {
            icon: "product vector catalog".to_owned(),
        });
    }
    let mut icons = BTreeMap::new();
    for source in source_icons {
        if source.name.is_empty() {
            return Err(GenerationError::MissingIcon {
                icon: "unnamed vector".to_owned(),
            });
        }
        if icons.contains_key(&source.name) {
            return Err(GenerationError::DuplicateIcon { icon: source.name });
        }
        let actual = sha256(source.svg.as_bytes());
        if source.checksum != actual {
            return Err(GenerationError::VectorChecksumMismatch {
                icon: source.name,
                expected: source.checksum,
                actual,
            });
        }
        let view_box = parse_svg_view_box(&source.name, &source.svg)?;
        icons.insert(
            source.name.clone(),
            VectorIcon {
                name: source.name,
                view_box,
                checksum: actual,
            },
        );
    }
    Ok(IconCatalog { icons })
}

fn parse_svg_view_box(name: &str, svg: &str) -> Result<String, GenerationError> {
    let svg = svg.trim();
    let root = svg
        .strip_prefix("<svg")
        .ok_or_else(|| invalid_svg(name, "root element must be svg"))?;
    if !root
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '>' | '/') || character.is_ascii_whitespace())
    {
        return Err(invalid_svg(name, "root element must be svg"));
    }
    if svg.contains("<image") {
        return Err(invalid_svg(name, "raster image elements are not allowed"));
    }
    let end = svg
        .find('>')
        .ok_or_else(|| invalid_svg(name, "unterminated svg tag"))?;
    let opening_tag = &svg[..=end];
    let view_box = svg_attribute(opening_tag, "viewBox")
        .ok_or_else(|| invalid_svg(name, "missing viewBox"))?;
    if view_box.split_ascii_whitespace().count() != 4
        || view_box
            .split_ascii_whitespace()
            .any(|dimension| dimension.parse::<f32>().is_err())
    {
        return Err(invalid_svg(name, "viewBox must contain four numbers"));
    }
    if ![
        "path", "circle", "rect", "line", "polyline", "polygon", "ellipse", "g",
    ]
    .iter()
    .any(|element| contains_svg_element(svg, element))
    {
        return Err(invalid_svg(name, "missing vector geometry"));
    }
    Ok(view_box.to_owned())
}

fn contains_svg_element(svg: &str, element: &str) -> bool {
    let marker = format!("<{element}");
    let mut offset = 0;
    while let Some(start) = svg[offset..].find(&marker) {
        let end = offset + start + marker.len();
        if svg[end..].chars().next().is_some_and(|character| {
            matches!(character, '>' | '/') || character.is_ascii_whitespace()
        }) {
            return true;
        }
        offset = end;
    }
    false
}

fn svg_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=");
    let mut offset = 0;
    let start = loop {
        let position = tag[offset..].find(&marker)? + offset;
        if tag[..position]
            .chars()
            .next_back()
            .is_some_and(|character| character == '<' || character.is_ascii_whitespace())
        {
            break position + marker.len();
        }
        offset = position + marker.len();
    };
    let quote = tag.as_bytes().get(start).copied()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let value_start = start + 1;
    let value_end = tag[value_start..].find(quote as char)? + value_start;
    Some(&tag[value_start..value_end])
}

fn invalid_svg(name: &str, message: &str) -> GenerationError {
    GenerationError::InvalidSvg {
        icon: name.to_owned(),
        message: message.to_owned(),
    }
}

fn verify_checksum(expected: &str, bytes: &[u8]) -> Result<(), (String, String)> {
    let actual = sha256(bytes);
    if expected == actual {
        Ok(())
    } else {
        Err((expected.to_owned(), actual))
    }
}

fn source_digest(token_json: &str, icons: &IconCatalog) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_json.as_bytes());
    for icon in icons.icons.values() {
        hasher.update(icon.name.as_bytes());
        hasher.update(icon.checksum.as_bytes());
    }
    hex_digest(hasher.finalize().as_slice())
}

fn render_rust(
    tokens: &BTreeMap<String, SemanticToken>,
    icons: &IconCatalog,
    source_digest: &str,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "// Generated by parchmint-design-system. Do not edit."
    )
    .expect("write to string");
    writeln!(output, "pub const SOURCE_DIGEST: &str = {source_digest:?};")
        .expect("write to string");
    writeln!(
        output,
        "pub const TOKENS: &[(&str, &str, &str, Option<&str>, Option<&str>)] = &["
    )
    .expect("write to string");
    for token in tokens.values() {
        writeln!(
            output,
            "    ({:?}, {:?}, {:?}, {:?}, {:?}),",
            token.name, token.token_type, token.value, token.light, token.dark
        )
        .expect("write to string");
    }
    writeln!(output, "];\npub const ICONS: &[(&str, &str, &str)] = &[").expect("write to string");
    for icon in icons.icons.values() {
        writeln!(
            output,
            "    ({:?}, {:?}, {:?}),",
            icon.name, icon.view_box, icon.checksum
        )
        .expect("write to string");
    }
    writeln!(output, "];\n").expect("write to string");
    output
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut digest = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(digest, "{byte:02x}").expect("write to string");
    }
    digest
}
