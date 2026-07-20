//! 配置 property tests：Property 3（往返）、23（幂等）、26（无效 TOML 报错）。
use proptest::prelude::*;
use rastery_core::config::{self, AppConfig, GenerationHistoryRecord, Language};
use rastery_core::format::{OutputFormat, PngCompression, Quality};
use std::path::PathBuf;

fn arb_config() -> impl Strategy<Value = AppConfig> {
    (0u8..3, 1u8..=100u8, 0u8..3, any::<bool>(), any::<bool>()).prop_map(
        |(fmt, q, png, lang, has_dir)| AppConfig {
            default_export_format: match fmt {
                0 => OutputFormat::Png,
                1 => OutputFormat::Jpeg,
                _ => OutputFormat::Webp,
            },
            default_export_quality: Quality::new(q).unwrap_or_default(),
            default_png_compression: match png {
                0 => PngCompression::Fast,
                1 => PngCompression::Default,
                _ => PngCompression::Best,
            },
            language: if lang { Language::En } else { Language::ZhCn },
            last_output_dir: if has_dir {
                Some(PathBuf::from("/tmp/rastery-out"))
            } else {
                None
            },
            default_provider: "seedream".to_string(),
            generation_history: Vec::new(),
        },
    )
}

proptest! {
    // Feature: rastery, Property 3: Configuration round-trip preserves data
    #[test]
    fn prop_config_roundtrip(c in arb_config()) {
        let toml_str = config::to_toml(&c).expect("serialize");
        let back = config::from_toml(&toml_str).expect("deserialize");
        prop_assert_eq!(back, c);
    }

    // Feature: rastery, Property 23: Configuration save-reload is idempotent
    #[test]
    fn prop_config_save_reload_idempotent(c in arb_config()) {
        let once = config::to_toml(&c).expect("s1");
        let reloaded = config::from_toml(&once).expect("reload");
        let twice = config::to_toml(&reloaded).expect("s2");
        prop_assert_eq!(once, twice);
    }
}

// Feature: rastery, Property 26: Invalid TOML returns error without crash
#[test]
fn invalid_toml_returns_error_without_crash() {
    let result = config::from_toml("this is = = not valid toml ==");
    assert!(result.is_err(), "invalid TOML must error, not panic");
}

#[test]
fn legacy_config_without_png_compression_uses_default() {
    let legacy = r#"
default_export_format = "jpeg"
default_export_quality = 80
language = "En"
"#;
    let parsed = config::from_toml(legacy).expect("legacy v1 config must remain readable");
    assert_eq!(parsed.default_png_compression, PngCompression::Default);
    assert_eq!(parsed.default_provider, "seedream");
    assert!(parsed.generation_history.is_empty());
}

#[test]
fn v2_config_contains_history_but_has_no_api_key_field() {
    let config = AppConfig {
        default_provider: "openai".into(),
        generation_history: vec![GenerationHistoryRecord {
            created_at_unix_ms: 42,
            provider: "openai".into(),
            feature: "feat-t2i".into(),
            output_count: 2,
            saved_paths: vec![PathBuf::from("output.png")],
        }],
        ..AppConfig::default()
    };
    let encoded = config::to_toml(&config).expect("serialize v2 config");
    assert!(encoded.contains("default_provider = \"openai\""));
    assert!(encoded.contains("generation_history"));
    assert!(!encoded.to_ascii_lowercase().contains("api_key"));
    assert!(!encoded.contains("provider-secret"));
}
