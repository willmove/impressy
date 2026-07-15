//! 配置 property tests：Property 3（往返）、23（幂等）、26（无效 TOML 报错）。
use proptest::prelude::*;
use rastery_core::config::{self, AppConfig, Hotkey, Language, Modifier};
use rastery_core::format::{OutputFormat, Quality};
use std::path::PathBuf;

fn arb_config() -> impl Strategy<Value = AppConfig> {
    (
        0u8..3,
        1u8..=100u8,
        "[a-zA-Z]{1,3}",
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(fmt, q, key, lang, has_dir)| AppConfig {
            default_export_format: match fmt {
                0 => OutputFormat::Png,
                1 => OutputFormat::Jpeg,
                _ => OutputFormat::Webp,
            },
            default_export_quality: Quality::new(q).unwrap_or_default(),
            screenshot_hotkey: Hotkey {
                modifiers: vec![Modifier::Ctrl, Modifier::Shift],
                key,
            },
            screenshot_compression: Quality::default(),
            language: if lang { Language::En } else { Language::ZhCn },
            last_output_dir: if has_dir {
                Some(PathBuf::from("/tmp/rastery-out"))
            } else {
                None
            },
        })
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
