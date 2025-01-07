use super::Route;

pub struct Score;

impl Score {
    pub fn calc_score(route: &Route) -> f64 {
        let mut prev_bearing: Option<f32> = None;
        let mut tot_bearing_diff: f64 = 0.;
        let mut len_m: f64 = 0.;

        for segment in route.iter() {
            let line_len: f64 = segment.get_line().borrow().get_len_m().into();
            len_m += line_len;

            let curr_bearing = segment.get_bearing();
            if let Some(prev_bearing) = prev_bearing {
                let bearing_diff = (prev_bearing - curr_bearing).abs() as f64;
                tot_bearing_diff += if bearing_diff >= 90. {
                    // assumption is that a 90 or more
                    // degree turn is a junction, not a curve
                    // we don't want junctions
                    0.
                } else {
                    bearing_diff
                };
            }
            prev_bearing = if segment.get_end_point().borrow().is_junction() {
                None
            } else if let Some(hw) = segment.get_line().borrow().tags.borrow().highway() {
                if hw == "residential" {
                    None
                } else {
                    Some(curr_bearing)
                }
            } else {
                Some(curr_bearing)
            };
        }

        tot_bearing_diff / len_m * 1000.
    }
}
