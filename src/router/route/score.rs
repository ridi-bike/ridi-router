use std::collections::HashMap;

use crate::router::rules::{RouterRules, RulesTagValueAction};

use super::Route;

pub struct Score;

fn scale_priority(priority: u8) -> f64 {
    priority as f64 / 255.0
}

fn get_rule_adjustment(
    bearing_diff: f64,
    tag: &Option<&smartstring::alias::String>,
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
    pub fn calc_score(route: &Route, rules: &RouterRules) -> f64 {
        let mut prev_bearing: Option<f32> = None;
        let mut tot_bearing_diff_adj: f64 = 0.;
        let mut len_m: f64 = 0.;

        for segment in route.iter() {
            let line_len: f64 = segment.get_line().borrow().get_len_m().into();
            len_m += line_len;

            let curr_bearing = segment.get_bearing();
            if let Some(prev_bearing) = prev_bearing {
                let bearing_diff = (prev_bearing - curr_bearing).abs() as f64;
                tot_bearing_diff_adj += if bearing_diff >= 90. {
                    // assumption is that a 90 or more
                    // degree turn is a junction, not a curve
                    // we don't want junctions
                    0.
                } else {
                    let mut adjusted = bearing_diff;
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &segment.get_line().borrow().tags.borrow().highway(),
                        &rules.highway,
                    );
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &segment.get_line().borrow().tags.borrow().surface(),
                        &rules.surface,
                    );
                    adjusted += get_rule_adjustment(
                        bearing_diff,
                        &segment.get_line().borrow().tags.borrow().smoothness(),
                        &rules.smoothness,
                    );
                    adjusted
                }
            }
            prev_bearing = if segment.get_end_point().borrow().is_junction() {
                None
            } else if let Some(hw) = segment.get_line().borrow().tags.borrow().highway() {
                if hw == "residential" || segment.get_end_point().borrow().residential_in_proximity
                {
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
