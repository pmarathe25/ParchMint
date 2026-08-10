//! Stable, framework-contained inputs for external visual verification.
//!
//! The elements returned here are the production-native launcher and project
//! surfaces. Fixture-only surfaces are intentionally not part of this API.

/// A production window surface that can be captured headlessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualTarget {
    Launcher,
    Project,
}

impl VisualTarget {
    pub const ALL: [Self; 2] = [Self::Launcher, Self::Project];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Launcher => "launcher",
            Self::Project => "project",
        }
    }
}

/// Appearance selected by the external capture tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualAppearance {
    Light,
    Dark,
}

impl VisualAppearance {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Catalog viewport dimensions and scale for a production target.
///
/// Both current targets use the 1440x900 logical verification viewport. The
/// viewport is independent of each native window's default launch settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualTargetSpec {
    pub target: VisualTarget,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    /// The composition is production-native; this does not describe the
    /// native window's default launch size.
    pub production_native: bool,
}

impl VisualTargetSpec {
    pub const fn physical_size(self) -> (u32, u32) {
        (self.width * self.scale, self.height * self.scale)
    }
}

pub const LAUNCHER_VISUAL_SPEC: VisualTargetSpec = VisualTargetSpec {
    target: VisualTarget::Launcher,
    width: 1440,
    height: 900,
    scale: 2,
    production_native: true,
};

pub const PROJECT_VISUAL_SPEC: VisualTargetSpec = VisualTargetSpec {
    target: VisualTarget::Project,
    width: 1440,
    height: 900,
    scale: 2,
    production_native: true,
};

pub const VISUAL_TARGET_SPECS: &[VisualTargetSpec] = &[LAUNCHER_VISUAL_SPEC, PROJECT_VISUAL_SPEC];

pub const fn visual_target_spec(target: VisualTarget) -> VisualTargetSpec {
    match target {
        VisualTarget::Launcher => LAUNCHER_VISUAL_SPEC,
        VisualTarget::Project => PROJECT_VISUAL_SPEC,
    }
}

/// Result metadata from a newly written capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualCapture {
    pub target: VisualTarget,
    pub appearance: VisualAppearance,
    pub renderer: &'static str,
    pub output_path: std::path::PathBuf,
}

/// Capture failure without exposing the headless renderer's error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualCaptureError(pub String);

impl std::fmt::Display for VisualCaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for VisualCaptureError {}

/// Captures one production-native target to a new PNG.
///
/// `output_stem` is a stem path. The renderer-specific output is
/// `<stem>-tiny-skia.png`. Existing output is rejected; comparison belongs to
/// the verification crate.
#[cfg(feature = "visual-verification")]
pub fn capture_visual(
    target: VisualTarget,
    appearance: VisualAppearance,
    output_stem: impl AsRef<std::path::Path>,
) -> Result<VisualCapture, VisualCaptureError> {
    use iced::{Settings, Size, Theme};
    use iced_test::Simulator;

    let output_stem = output_stem.as_ref();
    // Keep this path calculation aligned with `iced_test::Snapshot`: it uses
    // the supplied path's file stem, adds the renderer suffix, then sets PNG.
    let output_name = output_stem
        .file_stem()
        .map(std::ffi::OsStr::to_string_lossy)
        .unwrap_or_default();
    let output_path = output_stem
        .with_file_name(format!("{output_name}-tiny-skia"))
        .with_extension("png");
    if output_path.exists() {
        return Err(VisualCaptureError(format!(
            "capture output already exists: {}",
            output_path.display()
        )));
    }
    let spec = visual_target_spec(target);
    let element = match target {
        VisualTarget::Launcher => crate::native::NativeDesktop::verification_launcher_element(),
        VisualTarget::Project => crate::native::NativeDesktop::verification_project_element(),
    };
    let mut simulator = Simulator::<()>::with_size(
        Settings::default(),
        Size::new(spec.width as f32, spec.height as f32),
        element,
    );
    let theme = match appearance {
        VisualAppearance::Light => Theme::Light,
        VisualAppearance::Dark => Theme::Dark,
    };
    let snapshot = simulator
        .snapshot(&theme)
        .map_err(|error| VisualCaptureError(error.to_string()))?;
    snapshot
        .matches_image(output_stem)
        .map_err(|error| VisualCaptureError(error.to_string()))?;
    Ok(VisualCapture {
        target,
        appearance,
        renderer: "tiny-skia",
        output_path,
    })
}

#[cfg(all(test, feature = "visual-verification"))]
mod tests {
    use iced::{Settings, Size};
    use iced_test::Simulator;

    use super::*;

    #[test]
    fn production_targets_have_stable_names_and_2x_sizes() {
        assert_eq!(
            VisualTarget::ALL.map(VisualTarget::name),
            ["launcher", "project"]
        );
        assert_eq!(LAUNCHER_VISUAL_SPEC.physical_size(), (2880, 1800));
        assert_eq!(PROJECT_VISUAL_SPEC.physical_size(), (2880, 1800));
        assert!(
            VISUAL_TARGET_SPECS
                .iter()
                .all(|spec| spec.production_native)
        );
    }

    #[test]
    fn launcher_and_project_production_views_render_headlessly() {
        for target in VisualTarget::ALL {
            let spec = visual_target_spec(target);
            let mut simulator = Simulator::<()>::with_size(
                Settings::default(),
                Size::new(spec.width as f32, spec.height as f32),
                match target {
                    VisualTarget::Launcher => {
                        crate::native::NativeDesktop::verification_launcher_element()
                    }
                    VisualTarget::Project => {
                        crate::native::NativeDesktop::verification_project_element()
                    }
                },
            );
            simulator
                .snapshot(&iced::Theme::Light)
                .expect("production view should render headlessly");
        }
    }
}
