//! 输出命名 property tests：Property 19（顺序且唯一）。

use std::collections::HashSet;

use proptest::prelude::*;
use impressy_core::format::OutputFormat;
use impressy_core::naming;

proptest! {
    // Feature: impressy, Property 19: batch filenames are sequential and unique
    #[test]
    fn prop_batch_filenames_are_sequential_and_unique(
        stems in proptest::collection::vec(".*", 1..=100),
        format in prop_oneof![
            Just(OutputFormat::Png),
            Just(OutputFormat::Jpeg),
            Just(OutputFormat::Webp),
        ],
    ) {
        let names = stems
            .iter()
            .enumerate()
            .map(|(index, stem)| naming::batch_filename(stem, index, format))
            .collect::<Vec<_>>();
        let unique = names.iter().collect::<HashSet<_>>();
        prop_assert_eq!(unique.len(), names.len());
        for (index, name) in names.iter().enumerate() {
            let sequence = format!("-impressy-{}.", index + 1);
            prop_assert!(name.contains(&sequence), "filename must contain its sequence");
            prop_assert!(name.ends_with(format.extension()));
            prop_assert!(!name.contains('/') && !name.contains('\\'));
        }
    }
}

#[test]
fn ai_filenames_are_timestamped_sequential_and_extension_safe() {
    let first = naming::ai_filename(1_700_000_000_000, 1, ".PNG");
    let second = naming::ai_filename(1_700_000_000_000, 2, "../webp");
    assert_eq!(first, "impressy-ai-1700000000000-01.png");
    assert_eq!(second, "impressy-ai-1700000000000-02.webp");
}
