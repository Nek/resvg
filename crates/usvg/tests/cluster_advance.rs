// Copyright 2026 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(all(feature = "text", feature = "system-fonts"))]

use std::sync::Arc;
use usvg::{Node, Options, Tree};

#[test]
fn multi_glyph_clusters_retain_all_shaped_advances() {
    let data = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../resvg/tests/fonts/NotoSansDevanagari-Regular.ttf"
    ))
    .unwrap();
    let face = rustybuzz::Face::from_slice(&data, 0).unwrap();
    let mut database = usvg::fontdb::Database::new();
    database.load_font_data(data.clone());
    let options = Options {
        fontdb: Arc::new(database),
        ..Default::default()
    };
    let mut tested_multiple_advances = false;
    for source in ["कि", "क्षि", "किं", "स्त्री", "धर्म"] {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(source);
        let shaped = rustybuzz::shape(&face, &[], buffer);
        assert!(shaped.glyph_infos().iter().all(|g| g.glyph_id != 0));
        let mut start = 0;
        for range in shaped.glyph_infos().chunk_by(|a, b| a.cluster == b.cluster) {
            tested_multiple_advances |= shaped.glyph_positions()[start..start + range.len()]
                .iter()
                .filter(|p| p.x_advance > 0)
                .count()
                > 1;
            start += range.len();
        }
        let expected = shaped
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance as f64)
            .sum::<f64>()
            * 16.0
            / face.units_per_em() as f64;
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><text font-family="Noto Sans Devanagari" font-size="16" x="10" y="30">{source}</text></svg>"#
        );
        let tree = Tree::from_str(&svg, &options).unwrap();
        let text = tree
            .root()
            .children()
            .iter()
            .find_map(|node| {
                if let Node::Text(text) = node {
                    Some(text)
                } else {
                    None
                }
            })
            .unwrap();
        let actual = f64::from(text.bounding_box().width());
        assert!(
            (actual - expected).abs() < 0.001,
            "{source}: layout width {actual} must retain shaped advance {expected}"
        );
    }
    assert!(
        tested_multiple_advances,
        "the fixture must include multiple advancing glyphs in one cluster"
    );
}
