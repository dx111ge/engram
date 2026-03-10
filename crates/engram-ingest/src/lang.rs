/// Language detection implementations.

use crate::traits::LanguageDetector;
use crate::types::DetectedLanguage;

/// Fallback language detector that always returns a configured default.
pub struct DefaultLanguageDetector {
    default_code: String,
}

impl DefaultLanguageDetector {
    pub fn new(default_code: &str) -> Self {
        Self {
            default_code: default_code.into(),
        }
    }
}

impl Default for DefaultLanguageDetector {
    fn default() -> Self {
        Self::new("en")
    }
}

impl LanguageDetector for DefaultLanguageDetector {
    fn detect(&self, _text: &str) -> DetectedLanguage {
        DetectedLanguage {
            code: self.default_code.clone(),
            confidence: 0.0,
        }
    }
}

/// Language detector backed by the `whatlang` crate.
///
/// Lightweight, fast, and supports 80+ languages.
/// Feature-gated behind `lang-detect` feature.
#[cfg(feature = "lang-detect")]
pub struct WhatlangDetector;

#[cfg(feature = "lang-detect")]
impl WhatlangDetector {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "lang-detect")]
impl Default for WhatlangDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "lang-detect")]
impl LanguageDetector for WhatlangDetector {
    fn detect(&self, text: &str) -> DetectedLanguage {
        if text.trim().is_empty() {
            return DetectedLanguage {
                code: "und".into(),
                confidence: 0.0,
            };
        }

        match whatlang::detect(text) {
            Some(info) => {
                let code = info.lang().code().to_string();
                let confidence = info.confidence() as f32;
                DetectedLanguage { code, confidence }
            }
            None => DetectedLanguage {
                code: "und".into(),
                confidence: 0.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_detector_returns_configured_language() {
        let det = DefaultLanguageDetector::new("de");
        let result = det.detect("Hallo Welt");
        assert_eq!(result.code, "de");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn default_detector_defaults_to_english() {
        let det = DefaultLanguageDetector::default();
        let result = det.detect("anything");
        assert_eq!(result.code, "en");
    }

    #[cfg(feature = "lang-detect")]
    #[test]
    fn whatlang_detects_english() {
        let det = WhatlangDetector::new();
        let result = det.detect("This is a test of the English language detection system and it should work correctly");
        assert_eq!(result.code, "eng");
        assert!(result.confidence > 0.5);
    }

    #[cfg(feature = "lang-detect")]
    #[test]
    fn whatlang_detects_german() {
        let det = WhatlangDetector::new();
        let result = det.detect("Dies ist ein Test der deutschen Spracherkennung und sollte korrekt funktionieren");
        assert_eq!(result.code, "deu");
        assert!(result.confidence > 0.5);
    }

    #[cfg(feature = "lang-detect")]
    #[test]
    fn whatlang_handles_empty_text() {
        let det = WhatlangDetector::new();
        let result = det.detect("");
        assert_eq!(result.code, "und");
        assert_eq!(result.confidence, 0.0);
    }
}
