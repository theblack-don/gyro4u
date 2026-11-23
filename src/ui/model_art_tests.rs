//! Test helpers that keep the hand-drawn SVG art aligned with the layout
//! constants used for hit testing and overlay painting.

use roxmltree::{Document, Node};

fn parse(svg: &'static str) -> Document<'static> {
    Document::parse(svg).expect("SVG art parses")
}

fn element<'a, 'input>(doc: &'a Document<'input>, id: &str) -> Node<'a, 'input> {
    doc.descendants()
        .find(|node| node.is_element() && node.attribute("id") == Some(id))
        .unwrap_or_else(|| panic!("SVG element '{id}' not found"))
}

fn number(node: Node<'_, '_>, name: &str) -> f32 {
    node.attribute(name)
        .unwrap_or_else(|| panic!("<{}> is missing '{name}'", node.tag_name().name()))
        .parse()
        .unwrap_or_else(|_| panic!("<{}> has invalid '{name}'", node.tag_name().name()))
}

pub fn assert_viewbox(svg: &'static str, width: f32, height: f32) {
    let doc = parse(svg);
    let viewbox = doc
        .root_element()
        .attribute("viewBox")
        .expect("root element has a viewBox");
    let values: Vec<f32> = viewbox
        .split_whitespace()
        .map(|value| value.parse().expect("viewBox values are numeric"))
        .collect();
    assert_eq!(values.len(), 4, "viewBox has four values");
    assert!(
        (values[2] - width).abs() < 1e-3 && (values[3] - height).abs() < 1e-3,
        "viewBox {viewbox:?} does not match {width} x {height}"
    );
}

pub fn assert_rect(svg: &'static str, id: &str, x: f32, y: f32, w: f32, h: f32) {
    let doc = parse(svg);
    let node = element(&doc, id);
    assert_eq!(node.tag_name().name(), "rect", "'{id}' should be a rect");
    for (name, expected) in [("x", x), ("y", y), ("width", w), ("height", h)] {
        let actual = number(node, name);
        assert!(
            (actual - expected).abs() < 1e-3,
            "'{id}' {name} {actual} != {expected}"
        );
    }
}

pub fn assert_circle(svg: &'static str, id: &str, cx: f32, cy: f32, r: f32) {
    let doc = parse(svg);
    let node = element(&doc, id);
    assert_eq!(
        node.tag_name().name(),
        "circle",
        "'{id}' should be a circle"
    );
    for (name, expected) in [("cx", cx), ("cy", cy), ("r", r)] {
        let actual = number(node, name);
        assert!(
            (actual - expected).abs() < 1e-3,
            "'{id}' {name} {actual} != {expected}"
        );
    }
}

/// Bounding box of an absolute `M`/`L`/`C`/`Z` path.
pub fn assert_path_bounds(svg: &'static str, id: &str, x: f32, y: f32, w: f32, h: f32) {
    let doc = parse(svg);
    let node = element(&doc, id);
    assert_eq!(node.tag_name().name(), "path", "'{id}' should be a path");

    let d = node.attribute("d").expect("path has a d attribute");
    let mut tokens = d
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty());
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    while let Some(token) = tokens.next() {
        let pairs = match token {
            "M" | "L" => 1,
            "C" => 3,
            "Z" | "z" => 0,
            other => panic!("'{id}' uses an unsupported path command '{other}'"),
        };
        for _ in 0..pairs {
            let px: f32 = tokens
                .next()
                .expect("path x coordinate")
                .parse()
                .expect("numeric path x coordinate");
            let py: f32 = tokens
                .next()
                .expect("path y coordinate")
                .parse()
                .expect("numeric path y coordinate");
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
        }
    }

    for (name, actual, expected) in [
        ("x", min_x, x),
        ("y", min_y, y),
        ("width", max_x - min_x, w),
        ("height", max_y - min_y, h),
    ] {
        assert!(
            (actual - expected).abs() < 1e-3,
            "'{id}' {name} {actual} != {expected}"
        );
    }
}

pub fn assert_no_text(svg: &'static str) {
    let doc = parse(svg);
    assert!(
        doc.descendants()
            .all(|node| node.tag_name().name() != "text"),
        "SVG must not embed text; labels come from the egui overlay"
    );
}
