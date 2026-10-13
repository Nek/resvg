// Copyright 2026 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(all(feature = "text", feature = "system-fonts"))]

use std::sync::Arc;

use usvg::{Node, Options, Tree};

#[test]
fn computed_fonts_preserve_source_ranges_and_resolve_relative_units() {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../resvg/tests/fonts"));
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="100"><text x="1em" y="2em" style="font-size:0!important;font-family:missing!important">repeat<tspan font-size="50%">repeat</tspan></text></svg>"#;
    let xml = roxmltree::Document::parse(source).unwrap();
    let owner = xml.descendants().find(|n| n.has_tag_name("text")).unwrap();
    let mut options = Options {
        fontdb: Arc::new(fonts),
        ..Default::default()
    };
    let plain = Tree::from_str(source, &options).unwrap();
    assert!(
        plain.root().children().is_empty(),
        "zero-size text stays hidden without overrides"
    );
    options.computed_fonts.insert(
        owner.range().start,
        usvg::ComputedFontStyle {
            font_family: Some("Noto Sans".into()),
            font_size: Some("32px".into()),
            font_weight: Some("700".into()),
            font_style: Some("italic".into()),
        },
    );
    let tree = Tree::from_str(source, &options).unwrap();
    let Node::Text(text) = &tree.root().children()[0] else {
        panic!("text")
    };
    let chunk = &text.chunks()[0];
    assert_eq!(chunk.x(), Some(32.0));
    assert_eq!(chunk.y(), Some(64.0));
    assert_eq!(
        chunk
            .spans()
            .iter()
            .map(|s| s.font_size().get())
            .collect::<Vec<_>>(),
        [32.0, 16.0]
    );
    let origins: Vec<_> = xml
        .descendants()
        .filter(|n| n.is_text())
        .map(|n| n.range())
        .collect();
    assert_eq!(
        chunk
            .spans()
            .iter()
            .map(|s| s.source().unwrap().xml_range.clone())
            .collect::<Vec<_>>(),
        origins
    );
    for span in chunk.spans() {
        assert_eq!(span.font().weight(), 700);
        assert_eq!(span.font().style(), usvg::FontStyle::Italic);
    }
    assert!(
        Tree::from_data_nested(source.as_bytes(), &options)
            .unwrap()
            .root()
            .children()
            .is_empty(),
        "source offsets must not leak into separately loaded SVG documents"
    );
    let child = xml.descendants().find(|n| n.has_tag_name("tspan")).unwrap();
    options.computed_fonts.insert(
        child.range().start,
        usvg::ComputedFontStyle {
            font_size: Some("0px".into()),
            ..Default::default()
        },
    );
    let tree = Tree::from_str(source, &options).unwrap();
    let Node::Text(text) = &tree.root().children()[0] else {
        panic!("text")
    };
    assert_eq!(
        text.chunks()[0].spans().len(),
        1,
        "a computed zero size hides only its own occurrence"
    );
}

#[test]
fn resolved_fonts_bind_to_original_occurrences_without_matching_text() {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../resvg/tests/fonts"));
    let options = Options {
        fontdb: Arc::new(fonts),
        ..Default::default()
    };
    let mut multi_character_cluster = false;
    let mut reordered_cluster = false;
    for body in [
        r#"<text x="20" y="30">off<tspan>ice</tspan> e<tspan>́</tspan></text>"#,
        r#"<text x="20" y="30" xml:space="preserve"> A &amp; A <tspan><![CDATA[ <A> ]]></tspan></text>"#,
        r#"<text x="20" y="30">repeat <tspan font-weight="bold">repeat</tspan> repeat</text>"#,
        r#"<text x="20" y="30">office é <tspan font-style="italic">office é</tspan></text>"#,
        r#"<text x="20" y="30">مرحبا <tspan font-weight="bold">لا</tspan> مرحبا</text>"#,
        r#"<text x="20" y="30">Hel<tspan fill="red">lo של</tspan>ום.</text>"#,
        r#"<text x="20" y="30">  A   <tspan> A </tspan> A  </text>"#,
        r#"<text x="20" y="30">Alpha 😀 中文 <tspan font-weight="bold">Alpha 😀 中文</tspan></text>"#,
        r#"<text x="20 40 60" y="30">ABC<tspan x="20" y="60">ABC</tspan></text>"#,
    ] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="100" font-family="Noto Sans">{body}</svg>"#
        );
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let tree = Tree::from_str(&svg, &options).unwrap();
        let text = tree
            .root()
            .children()
            .iter()
            .find_map(|n| if let Node::Text(t) = n { Some(t) } else { None })
            .expect("text");
        let mut mapped = std::collections::BTreeSet::new();
        for chunk in text.chunks() {
            for span in chunk.spans() {
                let origin = span.source().expect("span source origin");
                assert!(
                    xml.descendants()
                        .any(|n| n.is_text() && n.range() == origin.xml_range)
                );
                assert_eq!(
                    &origin.text[origin.range.clone()],
                    &chunk.text()[span.start()..span.end()]
                );
                mapped.insert(origin.xml_range.start);
            }
        }
        assert_eq!(
            mapped.len(),
            xml.descendants().filter(|n| n.is_text()).count(),
            "{body}"
        );
        let mut previous = None;
        let mut coverage = std::collections::BTreeSet::new();
        for glyph in text.layouted().iter().flat_map(|s| &s.positioned_glyphs) {
            let chunk = &text.chunks()[glyph.chunk_index];
            let cluster = &chunk.text()[glyph.source_range.clone()];
            coverage.extend(
                cluster
                    .char_indices()
                    .map(|(offset, _)| (glyph.chunk_index, glyph.source_range.start + offset)),
            );
            multi_character_cluster |= cluster.chars().count() > 1;
            if let Some((chunk_index, start)) = previous {
                reordered_cluster |=
                    chunk_index == glyph.chunk_index && start > glyph.source_range.start;
            }
            previous = Some((glyph.chunk_index, glyph.source_range.start));
            assert!(!cluster.is_empty(), "empty cluster for {glyph:?}");
            assert!(
                cluster.contains(&glyph.text),
                "{cluster:?} does not cover {glyph:?}"
            );
            let span = chunk
                .spans()
                .iter()
                .find(|s| {
                    s.start() <= glyph.source_range.start && glyph.source_range.start < s.end()
                })
                .unwrap();
            assert!(span.source().is_some());
            assert!(tree.fontdb().face(glyph.font).is_some());
        }
        let expected: std::collections::BTreeSet<_> = text
            .chunks()
            .iter()
            .enumerate()
            .flat_map(|(index, chunk)| {
                chunk
                    .text()
                    .char_indices()
                    .map(move |(offset, _)| (index, offset))
            })
            .collect();
        assert_eq!(
            coverage, expected,
            "every normalized character must retain its source range: {body}"
        );
    }
    assert!(
        multi_character_cluster,
        "exercise shaped clusters, not just ASCII characters"
    );
    assert!(
        reordered_cluster,
        "exercise logical ranges in visual RTL order"
    );
}

#[test]
fn fallback_preserves_clusters_when_fonts_produce_different_glyph_counts() {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../resvg/tests/fonts"));
    let options = Options {
        fontdb: Arc::new(fonts),
        ..Default::default()
    };
    for source in ["office 漢 لا", "ffi e\u{301} 漢 العربية", "لا 漢 office"] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="100"><text x="20" y="30" font-family="Noto Sans">{source}</text></svg>"#
        );
        let tree = Tree::from_str(&svg, &options).unwrap();
        let Node::Text(text) = &tree.root().children()[0] else {
            panic!("text");
        };
        let mut covered = std::collections::BTreeSet::new();
        for glyph in text
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs)
        {
            assert_ne!(glyph.id.0, 0, "unresolved glyph in {source}: {glyph:?}");
            let run = &text.chunks()[glyph.chunk_index].text()[glyph.source_range.clone()];
            covered.extend(
                run.char_indices()
                    .map(|(offset, _)| glyph.source_range.start + offset),
            );
        }
        assert_eq!(
            covered,
            source.char_indices().map(|(offset, _)| offset).collect(),
            "fallback must retain every source cluster"
        );
    }
}
