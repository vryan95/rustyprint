//! `@color-profile` (CSS Color 5 § 9): ICC profiles that `color()` names. Colors in them are
//! converted to sRGB with `moxcms` for anything drawn as sRGB, and keep their components
//! so PDF output can use the profile itself.

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use moxcms::{
    ColorProfile as IccProfile, DataColorSpace, Layout, RenderingIntent, TransformF32Executor,
    TransformOptions,
};

/// A loaded color profile.
pub struct ColorProfile {
    /// Its `<dashed-ident>`, or `device-cmyk` for the profile of `device-cmyk()`.
    pub name: Arc<str>,
    /// The ICC profile.
    pub data: Arc<[u8]>,
    /// The number of color components: 1 (gray), 3 (RGB) or 4 (CMYK).
    pub components: usize,
    to_srgb: Arc<TransformF32Executor>,
}

impl fmt::Debug for ColorProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ColorProfile")
            .field("name", &self.name)
            .field("components", &self.components)
            .finish_non_exhaustive()
    }
}

/// Parses a `rendering-intent` descriptor value.
pub fn rendering_intent(value: &str) -> Option<RenderingIntent> {
    Some(match &*value.trim().to_ascii_lowercase() {
        "relative-colorimetric" => RenderingIntent::RelativeColorimetric,
        "absolute-colorimetric" => RenderingIntent::AbsoluteColorimetric,
        "perceptual" => RenderingIntent::Perceptual,
        "saturation" => RenderingIntent::Saturation,
        _ => return None,
    })
}

impl ColorProfile {
    /// Loads an ICC profile of gray, RGB or CMYK colors.
    pub fn load(name: &str, data: Vec<u8>, intent: RenderingIntent) -> Result<Self, String> {
        let icc = IccProfile::new_from_slice(&data).map_err(|_| "not an ICC profile".to_owned())?;
        let (layout, components) = match icc.color_space {
            DataColorSpace::Gray => (Layout::Gray, 1),
            DataColorSpace::Rgb => (Layout::Rgb, 3),
            DataColorSpace::Cmyk => (Layout::Rgba, 4),
            other => return Err(format!("{other:?} profiles are not supported")),
        };
        // A profile without tables for the intent uses its others, perceptual first
        // (ICC.1 § 8.10).
        let srgb = IccProfile::new_srgb();
        let mut result = Err(String::new());
        for intent in [
            intent,
            RenderingIntent::Perceptual,
            RenderingIntent::RelativeColorimetric,
        ] {
            let options = TransformOptions {
                rendering_intent: intent,
                ..TransformOptions::default()
            };
            result = icc
                .create_transform_f32(layout, &srgb, Layout::Rgb, options)
                .map_err(|error| format!("cannot convert its colors: {error:?}"));
            if result.is_ok() {
                break;
            }
        }
        let to_srgb = result?;
        Ok(Self {
            name: Arc::from(name),
            data: Arc::from(data),
            components,
            to_srgb,
        })
    }

    /// The color with `components` (from 0 to 1) in sRGB, beyond 0–1 for colors outside
    /// it. Missing components are 0.
    pub fn to_srgb(&self, components: &[f32]) -> [f32; 3] {
        let mut source = vec![0.0f32; self.components];
        for (to, from) in source.iter_mut().zip(components) {
            *to = from.clamp(0.0, 1.0);
        }
        let mut rgb = [0.0f32; 3];
        if self.to_srgb.transform(&source, &mut rgb).is_err() {
            return [0.0; 3];
        }
        rgb
    }
}

thread_local! {
    /// The profiles of the document being styled.
    static ACTIVE: RefCell<Option<Arc<[ColorProfile]>>> = const { RefCell::new(None) };
}

/// Runs `f` with `profiles` known to `color()`, as styling a document does.
pub(crate) fn with_profiles<T>(profiles: &Arc<[ColorProfile]>, f: impl FnOnce() -> T) -> T {
    let previous = ACTIVE.with(|active| active.replace(Some(profiles.clone())));
    let result = f();
    ACTIVE.with(|active| *active.borrow_mut() = previous);
    result
}

/// The sRGB color of `components` in the profile `name`, and the profile's index, if the
/// document has it.
pub(crate) fn convert(name: &str, components: &[f32]) -> Option<([f32; 3], u32)> {
    ACTIVE.with(|active| {
        let active = active.borrow();
        let profiles = active.as_ref()?;
        let index = profiles.iter().position(|p| &*p.name == name)?;
        Some((profiles[index].to_srgb(components), index as u32))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(file: &str) -> ColorProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/css-at-rules/assets")
            .join(file);
        let data = std::fs::read(path).expect("the test profile exists");
        ColorProfile::load("--test", data, RenderingIntent::RelativeColorimetric)
            .expect("the test profile loads")
    }

    fn close(actual: [f32; 3], expected: [f32; 3]) -> bool {
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() < 0.02)
    }

    #[test]
    fn converts_to_srgb() {
        let gray = load("gray.icc");
        assert_eq!(gray.components, 1);
        assert!(
            close(gray.to_srgb(&[0.5]), [0.735; 3]),
            "{:?}",
            gray.to_srgb(&[0.5])
        );
        let rgb = load("linear-rgb.icc");
        assert_eq!(rgb.components, 3);
        assert!(close(rgb.to_srgb(&[1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]));
        assert!(close(rgb.to_srgb(&[0.5, 0.5, 0.5]), [0.735; 3]));
        let cmyk = load("cmyk.icc");
        assert_eq!(cmyk.components, 4);
        assert!(
            close(cmyk.to_srgb(&[0.0; 4]), [1.0; 3]),
            "{:?}",
            cmyk.to_srgb(&[0.0; 4])
        );
        assert!(close(cmyk.to_srgb(&[0.0, 0.0, 0.0, 1.0]), [0.0; 3]));
        let cyan = cmyk.to_srgb(&[1.0, 0.0, 0.0, 0.0]);
        assert!(close(cyan, [0.0, 1.0, 1.0]), "{cyan:?}");
        assert!(ColorProfile::load("--bad", vec![1, 2, 3], RenderingIntent::Perceptual).is_err());
    }
}
