use std::collections::HashMap;

use smartstring::alias::String as SmartString;

use crate::{
    router::rules::{RouterRules, RulesTagValueAction},
    RoutingContext,
};

use super::Route;

/// Bearing changes are measured between chords at least this long, so the digitising jitter of
/// densely mapped straight roads does not count as corners.
const BEARING_WINDOW_M: f64 = 20.;
/// The route is scored in bins of about this length.
const BIN_M: f64 = 1000.;
/// A bin with less curvature than this is a straight. Median bin curvature on the paved Latvia
/// reference routes is ~40 deg/km, on the Slovak Carpathians ~100 deg/km.
const STRAIGHT_DEG_PER_KM: f64 = 30.;
/// An unbroken run of straight bins counts as `run_km ^ STRAIGHT_RUN_EXP` km, so one long slog
/// costs more than the same distance of short straights scattered between corners
/// (1 km -> 1, 5 km -> 11, 20 km -> 89).
const STRAIGHT_RUN_EXP: f64 = 1.5;

pub struct Score;

fn scale_priority(priority: u8) -> f64 {
    priority as f64 / 255.0
}

fn get_rule_adjustment(
    bearing_diff: f64,
    tag: Option<&SmartString>,
    rule: &Option<HashMap<String, RulesTagValueAction>>,
) -> f64 {
    if let Some(curr_tag) = tag {
        if let Some(tag_rules) = rule {
            return match tag_rules.get(curr_tag.as_str()) {
                Some(RulesTagValueAction::Priority {
                    value: priority_value,
                }) => bearing_diff * scale_priority(*priority_value),
                _ => 0.,
            };
        }
    }
    0.
}

/// Smallest angle between two bearings in degrees (0..=180).
fn bearing_diff(a: f64, b: f64) -> f64 {
    let diff = (a - b).abs() % 360.;
    if diff > 180. {
        360. - diff
    } else {
        diff
    }
}

/// Folds ~1 km bins into a score: total degrees over effective length, where runs of straight
/// bins are stretched by `STRAIGHT_RUN_EXP`.
#[derive(Default)]
struct BinScorer {
    bin_len_m: f64,
    bin_deg: f64,
    tot_deg: f64,
    eff_len_km: f64,
    straight_run_km: f64,
}

impl BinScorer {
    fn add(&mut self, len_m: f64, deg: f64) {
        self.bin_len_m += len_m;
        self.bin_deg += deg;
        if self.bin_len_m >= BIN_M {
            self.close_bin();
        }
    }

    fn close_bin(&mut self) {
        if self.bin_len_m <= 0. {
            return;
        }
        let len_km = self.bin_len_m / 1000.;
        if self.bin_deg / len_km < STRAIGHT_DEG_PER_KM {
            self.straight_run_km += len_km;
        } else {
            self.close_straight_run();
            self.eff_len_km += len_km;
        }
        self.tot_deg += self.bin_deg;
        self.bin_len_m = 0.;
        self.bin_deg = 0.;
    }

    fn close_straight_run(&mut self) {
        self.eff_len_km += self.straight_run_km.powf(STRAIGHT_RUN_EXP);
        self.straight_run_km = 0.;
    }

    fn finish(mut self) -> f64 {
        self.close_bin();
        self.close_straight_run();
        if self.eff_len_km > 0. {
            self.tot_deg / self.eff_len_km
        } else {
            0.
        }
    }
}

impl Score {
    pub fn calc_score(ctx: &RoutingContext<'_>, route: &Route, rules: &RouterRules) -> f64 {
        let mut scorer = BinScorer::default();
        let mut anchor = None;
        let mut window_len_m: f64 = 0.;
        let mut prev_bearing: Option<f64> = None;

        for segment in route.iter() {
            let line = ctx.line(segment.get_line());
            let end_point = segment.get_end_point();
            let line_len: f64 = ctx.point_distance_m(&line.points.0, &line.points.1).into();
            let anchor_point = anchor.get_or_insert_with(|| {
                if line.points.0 == *end_point {
                    line.points.1.clone()
                } else {
                    line.points.0.clone()
                }
            });
            window_len_m += line_len;

            let mut deg = 0.;
            if window_len_m >= BEARING_WINDOW_M {
                let curr_bearing = ctx.point_bearing(anchor_point, end_point) as f64;
                if let Some(prev_bearing) = prev_bearing {
                    let bearing_diff = bearing_diff(prev_bearing, curr_bearing);
                    let line_tags = ctx.tag_set(&line.tags);
                    deg = bearing_diff;
                    deg += ctx.with_tag_value(&line_tags.highway, |highway| {
                        get_rule_adjustment(bearing_diff, highway, &rules.highway)
                    });
                    deg += ctx.with_tag_value(&line_tags.surface, |surface| {
                        get_rule_adjustment(bearing_diff, surface, &rules.surface)
                    });
                    deg += ctx.with_tag_value(&line_tags.smoothness, |smoothness| {
                        get_rule_adjustment(bearing_diff, smoothness, &rules.smoothness)
                    });
                }
                prev_bearing = Some(curr_bearing);
                anchor = Some(end_point.clone());
                window_len_m = 0.;
            }
            scorer.add(line_len, deg);

            // turns at junctions and in towns are not corners
            let line_tags = ctx.tag_set(&line.tags);
            let reset = ctx.point_is_junction(end_point)
                || ctx.with_tag_value(&line_tags.highway, |highway| {
                    matches!(highway, Some(hw) if hw == "residential")
                })
                || ctx.point_near_residential(end_point);
            if reset {
                prev_bearing = None;
                anchor = Some(end_point.clone());
                window_len_m = 0.;
            }
        }

        scorer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score_bins(bins: &[(f64, f64)]) -> f64 {
        let mut scorer = BinScorer::default();
        for &(len_m, deg) in bins {
            scorer.add(len_m, deg);
        }
        scorer.finish()
    }

    #[test]
    fn bearing_diff_wraps_around_north() {
        assert_eq!(bearing_diff(350., 10.), 20.);
        assert_eq!(bearing_diff(10., 350.), 20.);
        assert_eq!(bearing_diff(90., 270.), 180.);
        assert_eq!(bearing_diff(45., 60.), 15.);
    }

    #[test]
    fn all_curvy_route_scores_degrees_per_km() {
        let bins = vec![(1000., 200.); 10];
        assert!((score_bins(&bins) - 200.).abs() < 1e-9);
    }

    #[test]
    fn short_straight_bins_count_at_face_value() {
        // 1 km straight runs between corners are not stretched
        let bins: Vec<_> = (0..10)
            .map(|i| if i % 2 == 0 { (1000., 200.) } else { (1000., 0.) })
            .collect();
        assert!((score_bins(&bins) - 100.).abs() < 1e-9);
    }

    #[test]
    fn one_long_straight_scores_lower_than_scattered_straights() {
        let scattered: Vec<_> = (0..20)
            .map(|i| if i % 2 == 0 { (1000., 200.) } else { (1000., 0.) })
            .collect();
        let slog: Vec<_> = (0..20)
            .map(|i| if i < 10 { (1000., 200.) } else { (1000., 0.) })
            .collect();
        assert!(score_bins(&slog) < score_bins(&scattered) / 2.);
    }

    #[test]
    fn straight_bins_add_no_score_and_empty_route_is_zero() {
        assert_eq!(score_bins(&[]), 0.);
        assert_eq!(score_bins(&[(1000., 0.); 5]), 0.);
    }
}
