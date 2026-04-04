use std::collections::HashMap;


use crate::{router::rules::{RouterRules, RulesTagValueAction}, RoutingContext};

use super::Route;

pub struct Score;

fn scale_priority(priority: u8) -> f64 {
    priority as f64 / 255.0
}

fn get_rule_adjustment(
    bearing_diff: f64,
    tag: &Option<String>,
    rule: &Option<HashMap<String, RulesTagValueAction>>,
) -> f64 {
    if let Some(ref curr_tag) = tag {
        if let Some(ref tag_rules) = rule {
            if let Some(curr_tag_rule) = tag_rules.get(curr_tag.as_str()) {
                if let RulesTagValueAction::Priority {
                    value: priority_value,
                } = curr_tag_rule
                {
                    return bearing_diff * scale_priority(*priority_value);
                }
            }
        }
    }
    0.
}

impl Score {
    pub fn calc_score(
        ctx: &RoutingContext<'_>,
        route: &Route,
        rules: &RouterRules,
    ) -> f64 {
        let mut prev_bearing: Option<f32> = None;
        let mut tot_bearing_diff_adj: f64 = 0.;
        let mut len_m: f64 = 0.;

        for segment in route.iter() {
            let line = ctx.line(segment.get_line());
            let point_a = ctx.point(&line.points.0);
            let point_b = ctx.point(&line.points.1);
            let line_len: f64 = line.len_m(&point_a, &point_b).into();
            len_m += line_len;

            let curr_bearing = segment.bearing(ctx);
            if let Some(prev_bearing) = prev_bearing {
                let bearing_diff = (prev_bearing - curr_bearing).abs() as f64;
                tot_bearing_diff_adj += if bearing_diff >= 90. {
                    0.
                } else {
                    let line_tags = ctx.tag_set(&line.tags);
                    let mut adjusted = bearing_diff;
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &ctx.tag_value(&line_tags.highway),
                        &rules.highway,
                    );
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &ctx.tag_value(&line_tags.surface),
                        &rules.surface,
                    );
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &ctx.tag_value(&line_tags.smoothness),
                        &rules.smoothness,
                    );
                    adjusted
                }
            }
            let end_point = ctx.point(segment.get_end_point());
            let line_tags = ctx.tag_set(&line.tags);
            prev_bearing = if end_point.is_junction() {
                None
            } else if let Some(hw) = ctx.tag_value(&line_tags.highway) {
                if hw == "residential" || end_point.residential_in_proximity {
                    None
                } else {
                    Some(curr_bearing)
                }
            } else {
                Some(curr_bearing)
            };
        }

        tot_bearing_diff_adj / len_m * 1000.
    }
}
