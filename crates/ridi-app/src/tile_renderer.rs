use i_triangle::float::triangulatable::Triangulatable;
use macroquad::prelude::*;

use crate::decoded_mvt_tile::{DecodedMvtTile, MvtFeature};
use crate::map_rendering_spec::{
    CircleStyle, DottedFillStyle, FeatureFilter, FillStyle, LineStyle, MapRenderingSpec,
    RenderInstruction, TextStyle, TextTransform,
};
use crate::mvt_geometry::MvtGeometryLines;
use crate::space::{GpsCoord, Space};
use crate::tile_address::TileAddress;

pub fn draw_map_tiles<'a>(
    tiles: impl IntoIterator<Item = &'a DecodedMvtTile>,
    space: &Space,
    spec: &MapRenderingSpec,
    label_font: Option<&Font>,
) {
    let mut tiles: Vec<_> = tiles.into_iter().collect();
    tiles.sort_by_key(|tile| (tile.address.z, tile.address.y, tile.address.x));

    let Some(first_tile) = tiles.first() else {
        return;
    };

    // Render by style rule across every visible tile, not tile-by-tile.
    //
    // Tile-by-tile rendering causes later tiles' polygon fills to overdraw
    // earlier tiles' roads and borders. That shows up as missing line
    // segments at zoom levels with multiple visible tiles.
    for rule in spec.rules_for_zoom(first_tile.address.z) {
        for tile in &tiles {
            if tile.address.z != first_tile.address.z {
                continue;
            }

            for feature in tile
                .features_in_layer(&rule.layer)
                .filter(|feature| matches_filter(*feature, &rule.filter))
            {
                match &rule.render {
                    RenderInstruction::Fill(style) => {
                        draw_styled_feature_fill(tile.address, feature, space, style);
                    }
                    RenderInstruction::Line(style) => {
                        draw_styled_feature_lines(tile.address, feature, space, style);
                    }
                    RenderInstruction::Circle(style) => {
                        draw_styled_feature_points(tile.address, feature, space, style);
                    }
                    RenderInstruction::Text(style) => {
                        draw_feature_text(tile.address, feature, space, style, label_font);
                    }
                }
            }
        }
    }
}

fn matches_filter(feature: MvtFeature<'_>, filter: &FeatureFilter) -> bool {
    if let Some(expected) = &filter.kind {
        if string_property(feature, "kind") != Some(expected.as_str()) {
            return false;
        }
    }

    if let Some(expected_values) = &filter.kind_any {
        let Some(actual) = string_property(feature, "kind") else {
            return false;
        };
        if !expected_values.iter().any(|expected| expected == actual) {
            return false;
        }
    }

    if let Some(excluded) = &filter.kind_not {
        if string_property(feature, "kind") == Some(excluded.as_str()) {
            return false;
        }
    }

    if let Some(expected) = &filter.kind_detail {
        if string_property(feature, "kind_detail") != Some(expected.as_str()) {
            return false;
        }
    }

    if let Some(expected_values) = &filter.kind_detail_any {
        let Some(actual) = string_property(feature, "kind_detail") else {
            return false;
        };
        if !expected_values.iter().any(|expected| expected == actual) {
            return false;
        }
    }

    true
}

fn string_property<'a>(feature: MvtFeature<'a>, key: &str) -> Option<&'a str> {
    feature.properties().find_map(|property| {
        if property.key() == key {
            property.value().as_str()
        } else {
            None
        }
    })
}

fn draw_styled_feature_lines(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    style: &LineStyle,
) {
    if let Some(casing_color) = style.casing_color() {
        draw_feature_lines(
            address,
            feature,
            space,
            casing_color,
            style.width_px + style.casing_width_px.unwrap_or(0.0) * 2.0,
            None,
        );
    }
    draw_feature_lines(
        address,
        feature,
        space,
        style.color(),
        style.width_px,
        style.dash.as_deref(),
    );
}

fn draw_styled_feature_points(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    style: &CircleStyle,
) {
    if let Some(stroke_color) = style.stroke_color() {
        draw_feature_points(
            address,
            feature,
            space,
            stroke_color,
            style.radius_px + style.stroke_width_px.unwrap_or(0.0),
        );
    }
    draw_feature_points(address, feature, space, style.color(), style.radius_px);
}

fn draw_styled_feature_fill(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    style: &FillStyle,
) {
    let mut lines = MvtGeometryLines::default();
    if feature.process_geometry(&mut lines).is_err() {
        return;
    }
    let extent = feature.extent() as f64;
    let fill_color = style.color();

    let mut triangles = Vec::new();

    for line in lines.lines() {
        if line.len() < 3 {
            continue;
        }

        let contour: Vec<[f64; 2]> = line
            .into_iter()
            .map(|point| {
                let point = mvt_coord_to_gps(address, point.0, point.1, extent);
                let point = space.world_to_screen(point);
                [point.x, point.y]
            })
            .collect();
        let triangulation = vec![contour].triangulate().to_triangulation::<u32>();

        for triangle in triangulation.indices.chunks_exact(3) {
            let a = triangulation.points[triangle[0] as usize];
            let b = triangulation.points[triangle[1] as usize];
            let c = triangulation.points[triangle[2] as usize];
            triangles.push((
                Vec2::new(a[0] as f32, a[1] as f32),
                Vec2::new(b[0] as f32, b[1] as f32),
                Vec2::new(c[0] as f32, c[1] as f32),
            ));
        }
    }

    if let Some(color) = fill_color {
        for &(a, b, c) in &triangles {
            draw_triangle(a, b, c, color);
        }
    }

    if let Some(dots) = &style.dots {
        draw_dotted_fill_triangles(&triangles, dots);
    }
}

fn draw_dotted_fill_triangles(triangles: &[(Vec2, Vec2, Vec2)], dots: &DottedFillStyle) {
    if triangles.is_empty() {
        return;
    }

    let spacing = dots.spacing_px.max(1.0);
    let radius = dots.radius_px.max(0.1);
    let color = dots.color();

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for &(a, b, c) in triangles {
        min_x = min_x.min(a.x).min(b.x).min(c.x);
        max_x = max_x.max(a.x).max(b.x).max(c.x);
        min_y = min_y.min(a.y).min(b.y).min(c.y);
        max_y = max_y.max(a.y).max(b.y).max(c.y);
    }

    let start_x = (min_x / spacing).floor() * spacing;
    let start_y = (min_y / spacing).floor() * spacing;

    let mut y = start_y;
    while y <= max_y {
        let mut x = start_x;
        while x <= max_x {
            let point = Vec2::new(x, y);
            if triangles
                .iter()
                .any(|&(a, b, c)| point_in_triangle(point, a, b, c))
            {
                draw_circle(x, y, radius, color);
            }
            x += spacing;
        }
        y += spacing;
    }
}

fn point_in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = triangle_sign(point, a, b);
    let d2 = triangle_sign(point, b, c);
    let d3 = triangle_sign(point, c, a);

    let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;

    !(has_negative && has_positive)
}

fn triangle_sign(p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
}

fn draw_feature_lines(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    color: Color,
    thickness: f32,
    dash: Option<&[f32]>,
) {
    let mut lines = MvtGeometryLines::default();
    if feature.process_geometry(&mut lines).is_err() {
        return;
    }
    let extent = feature.extent() as f64;

    for line in lines.lines() {
        for points in line.windows(2) {
            let from = mvt_coord_to_gps(address, points[0].0, points[0].1, extent);
            let to = mvt_coord_to_gps(address, points[1].0, points[1].1, extent);
            let from = space.world_to_screen(from);
            let to = space.world_to_screen(to);
            draw_line_with_optional_dash(from.x, from.y, to.x, to.y, thickness, color, dash);
        }
    }
}

fn draw_line_with_optional_dash(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    thickness: f32,
    color: Color,
    dash: Option<&[f32]>,
) {
    let Some(dash) = dash else {
        draw_line(
            from_x as f32,
            from_y as f32,
            to_x as f32,
            to_y as f32,
            thickness,
            color,
        );
        return;
    };

    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f64::EPSILON {
        return;
    }

    let dash_len = dash.first().copied().unwrap_or(0.0).max(0.0) as f64;
    let gap_len = dash.get(1).copied().unwrap_or(dash_len as f32).max(0.1) as f64;

    if dash_len <= f64::EPSILON {
        draw_dotted_line(from_x, from_y, dx, dy, length, thickness, gap_len, color);
    } else {
        draw_dashed_line(
            from_x, from_y, dx, dy, length, thickness, dash_len, gap_len, color,
        );
    }
}

fn draw_dotted_line(
    from_x: f64,
    from_y: f64,
    dx: f64,
    dy: f64,
    length: f64,
    thickness: f32,
    spacing: f64,
    color: Color,
) {
    let radius = (thickness / 2.0).max(1.0);
    let spacing = spacing.max(radius as f64 * 2.0);
    let mut distance = 0.0;

    while distance <= length {
        let t = distance / length;
        draw_circle(
            (from_x + dx * t) as f32,
            (from_y + dy * t) as f32,
            radius,
            color,
        );
        distance += spacing;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_dashed_line(
    from_x: f64,
    from_y: f64,
    dx: f64,
    dy: f64,
    length: f64,
    thickness: f32,
    dash_len: f64,
    gap_len: f64,
    color: Color,
) {
    let cycle = dash_len + gap_len;
    let mut distance = 0.0;

    while distance < length {
        let dash_end = (distance + dash_len).min(length);
        let start_t = distance / length;
        let end_t = dash_end / length;

        draw_line(
            (from_x + dx * start_t) as f32,
            (from_y + dy * start_t) as f32,
            (from_x + dx * end_t) as f32,
            (from_y + dy * end_t) as f32,
            thickness,
            color,
        );

        distance += cycle;
    }
}

fn draw_feature_points(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    color: Color,
    radius: f32,
) {
    let mut lines = MvtGeometryLines::default();
    if feature.process_geometry(&mut lines).is_err() {
        return;
    }
    let extent = feature.extent() as f64;

    for line in lines.lines() {
        for point in line {
            let point = mvt_coord_to_gps(address, point.0, point.1, extent);
            let point = space.world_to_screen(point);
            draw_circle(point.x as f32, point.y as f32, radius, color);
        }
    }
}

fn draw_feature_text(
    address: TileAddress,
    feature: MvtFeature<'_>,
    space: &Space,
    style: &TextStyle,
    label_font: Option<&Font>,
) {
    let Some(label) = label_for_feature(feature, style) else {
        return;
    };

    let mut lines = MvtGeometryLines::default();
    if feature.process_geometry(&mut lines).is_err() {
        return;
    }
    let extent = feature.extent() as f64;

    let offset = style
        .offset_px
        .unwrap_or(crate::map_rendering_spec::PixelOffset { x: 0.0, y: 0.0 });
    for line in lines.lines() {
        for point in line {
            let point = mvt_coord_to_gps(address, point.0, point.1, extent);
            let point = space.world_to_screen(point);
            let x = point.x as f32 + offset.x;
            let y = point.y as f32 + offset.y;

            if let Some(halo_color) = style.halo_color() {
                let halo = style.halo_width_px.unwrap_or(1.0);
                for (dx, dy) in [(-halo, 0.0), (halo, 0.0), (0.0, -halo), (0.0, halo)] {
                    draw_text_with_style(&label, x + dx, y + dy, style, halo_color, label_font);
                }
            }
            draw_text_with_style(&label, x, y, style, style.color(), label_font);
        }
    }
}

fn label_for_feature(feature: MvtFeature<'_>, style: &TextStyle) -> Option<String> {
    let label = style
        .fields
        .as_deref()
        .unwrap_or_else(|| {
            std::slice::from_ref(
                style
                    .field
                    .as_ref()
                    .expect("TextStyle requires field or fields"),
            )
        })
        .iter()
        .find_map(|field| string_property(feature, field))?;

    Some(match style.transform {
        Some(TextTransform::Uppercase) => label.to_uppercase(),
        None => label.to_owned(),
    })
}

fn draw_text_with_style(
    label: &str,
    x: f32,
    y: f32,
    style: &TextStyle,
    color: Color,
    label_font: Option<&Font>,
) {
    draw_text_ex(
        label,
        x,
        y,
        TextParams {
            font: label_font,
            font_size: style.size_px.round().clamp(1.0, u16::MAX as f32) as u16,
            color,
            ..Default::default()
        },
    );
}

fn mvt_coord_to_gps(address: TileAddress, x: f64, y: f64, extent: f64) -> GpsCoord {
    let n = 2.0_f64.powi(address.z as i32);
    let world_x = address.x as f64 + x / extent;
    let world_y = address.y as f64 + y / extent;

    let lon = world_x / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * world_y / n))
        .sinh()
        .atan();
    let lat = lat_rad.to_degrees();

    GpsCoord { lat, lon }
}
