use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    validate_locales(&manifest_dir);

    let icon = manifest_dir.join("assets/icons/rastery.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    assert!(
        icon.is_file(),
        "Rastery Windows icon is missing; regenerate the application icons"
    );

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let icon = icon
        .to_str()
        .expect("Rastery Windows icon path must be valid UTF-8");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon)
        .set("ProductName", "Rastery")
        .set("FileDescription", "Rastery local image processing toolbox")
        .set("OriginalFilename", "rastery.exe");
    resource
        .compile()
        .expect("failed to embed the Rastery Windows icon");
}

fn validate_locales(manifest_dir: &Path) {
    let locale_path = manifest_dir.join("locales/app.yml");
    let source_dir = manifest_dir.join("src");
    println!("cargo:rerun-if-changed={}", locale_path.display());
    println!("cargo:rerun-if-changed={}", source_dir.display());

    let source = fs::read_to_string(&locale_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", locale_path.display()));
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut languages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for raw_line in source.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = raw_key.trim().trim_matches(['\'', '"']);
        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }

        if matches!(key, "en" | "zh-CN") {
            let full_key = stack
                .iter()
                .map(|(_, part)| part.as_str())
                .collect::<Vec<_>>()
                .join(".");
            assert!(
                !full_key.is_empty() && !raw_value.trim().is_empty(),
                "locale entry {full_key}.{key} must not be empty"
            );
            languages
                .entry(full_key)
                .or_default()
                .insert(key.to_string());
        } else if raw_value.trim().is_empty() {
            stack.push((indent, key.to_string()));
        }
    }

    let mut locale_keys = BTreeSet::new();
    for (key, present) in languages {
        assert!(
            present.contains("en") && present.contains("zh-CN"),
            "locale key {key} must define both en and zh-CN"
        );
        locale_keys.insert(key);
    }

    let mut source_files = Vec::new();
    collect_rust_files(&source_dir, &mut source_files);
    for path in source_files {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for key in i18n_string_literals(&text) {
            assert!(
                locale_keys.contains(&key),
                "missing i18n key {key} referenced by {}",
                path.display()
            );
        }
    }
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
    {
        let path = entry
            .expect("source directory entry must be readable")
            .path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn i18n_string_literals(source: &str) -> BTreeSet<String> {
    const PREFIXES: &[&str] = &[
        "action.",
        "app.",
        "dialog.",
        "error.",
        "exif.",
        "feature.",
        "help.",
        "lang.",
        "nav.",
        "option.",
        "parameter.",
        "placeholder.",
        "settings.",
        "status.",
        "validation.",
    ];
    let mut result = BTreeSet::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, character)) = chars.next() {
        if character != '"' {
            continue;
        }
        let mut literal = String::new();
        let mut escaped = false;
        for (_, next) in chars.by_ref() {
            if escaped {
                escaped = false;
                literal.push(next);
            } else if next == '\\' {
                escaped = true;
            } else if next == '"' {
                break;
            } else {
                literal.push(next);
            }
        }
        let is_key = PREFIXES.iter().any(|prefix| literal.starts_with(prefix))
            && literal.chars().all(|value| {
                value.is_ascii_lowercase() || value.is_ascii_digit() || "._-".contains(value)
            });
        if is_key {
            result.insert(literal);
        }
    }
    result
}
