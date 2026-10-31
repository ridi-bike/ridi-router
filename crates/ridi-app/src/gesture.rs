use crate::space::ScreenCoord;
use macroquad::prelude::{Touch, TouchPhase};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchGesture {
    Drag {
        previous: ScreenCoord,
        current: ScreenCoord,
    },
    Pinch {
        previous_center: ScreenCoord,
        current_center: ScreenCoord,
        zoom: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TouchGestureState {
    previous: Option<TouchSnapshot>,
}

impl TouchGestureState {
    pub fn update(&mut self, touches: &[Touch]) -> Option<TouchGesture> {
        let current = TouchSnapshot::from_touches(touches);
        let gesture = self
            .previous
            .as_ref()
            .zip(current.as_ref())
            .and_then(|(previous, current)| previous.gesture_to(current));

        self.previous = current;
        gesture
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TouchSnapshot {
    One {
        id: u64,
        position: ScreenCoord,
    },
    Two {
        first_id: u64,
        first_position: ScreenCoord,
        second_id: u64,
        second_position: ScreenCoord,
    },
}

impl TouchSnapshot {
    fn from_touches(touches: &[Touch]) -> Option<Self> {
        let mut active_touches: Vec<_> = touches
            .iter()
            .filter(|touch| {
                matches!(
                    touch.phase,
                    TouchPhase::Started | TouchPhase::Stationary | TouchPhase::Moved
                )
            })
            .collect();

        active_touches.sort_by_key(|touch| touch.id);

        match active_touches.as_slice() {
            [touch] => Some(Self::One {
                id: touch.id,
                position: screen_coord(touch),
            }),
            [first, second, ..] => Some(Self::Two {
                first_id: first.id,
                first_position: screen_coord(first),
                second_id: second.id,
                second_position: screen_coord(second),
            }),
            [] => None,
        }
    }

    fn gesture_to(&self, current: &Self) -> Option<TouchGesture> {
        match (self, current) {
            (
                Self::One {
                    id: previous_id,
                    position: previous,
                },
                Self::One {
                    id: current_id,
                    position: current,
                },
            ) if previous_id == current_id => Some(TouchGesture::Drag {
                previous: *previous,
                current: *current,
            }),
            (
                Self::Two {
                    first_id: previous_first_id,
                    first_position: previous_first,
                    second_id: previous_second_id,
                    second_position: previous_second,
                },
                Self::Two {
                    first_id: current_first_id,
                    first_position: current_first,
                    second_id: current_second_id,
                    second_position: current_second,
                },
            ) if previous_first_id == current_first_id
                && previous_second_id == current_second_id =>
            {
                let previous_distance = distance(*previous_first, *previous_second);
                let current_distance = distance(*current_first, *current_second);

                if previous_distance <= f64::EPSILON || current_distance <= f64::EPSILON {
                    return None;
                }

                Some(TouchGesture::Pinch {
                    previous_center: midpoint(*previous_first, *previous_second),
                    current_center: midpoint(*current_first, *current_second),
                    zoom: previous_distance / current_distance,
                })
            }
            _ => None,
        }
    }
}

fn screen_coord(touch: &Touch) -> ScreenCoord {
    ScreenCoord {
        x: touch.position.x as f64,
        y: touch.position.y as f64,
    }
}

fn midpoint(first: ScreenCoord, second: ScreenCoord) -> ScreenCoord {
    ScreenCoord {
        x: (first.x + second.x) / 2.0,
        y: (first.y + second.y) / 2.0,
    }
}

fn distance(first: ScreenCoord, second: ScreenCoord) -> f64 {
    let dx = first.x - second.x;
    let dy = first.y - second.y;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::Vec2;

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch {
            id,
            phase,
            position: Vec2::new(x, y),
        }
    }

    #[test]
    fn one_active_touch_drags_between_frames() {
        let mut state = TouchGestureState::default();
        assert_eq!(
            state.update(&[touch(7, TouchPhase::Started, 10.0, 20.0)]),
            None
        );

        assert_eq!(
            state.update(&[touch(7, TouchPhase::Moved, 15.0, 30.0)]),
            Some(TouchGesture::Drag {
                previous: ScreenCoord { x: 10.0, y: 20.0 },
                current: ScreenCoord { x: 15.0, y: 30.0 },
            })
        );
    }

    #[test]
    fn ended_touches_clear_drag_state() {
        let mut state = TouchGestureState::default();
        state.update(&[touch(7, TouchPhase::Started, 10.0, 20.0)]);
        assert_eq!(
            state.update(&[touch(7, TouchPhase::Ended, 15.0, 30.0)]),
            None
        );
        assert_eq!(
            state.update(&[touch(8, TouchPhase::Started, 1.0, 2.0)]),
            None
        );
    }

    #[test]
    fn two_active_touches_pinch_between_frames() {
        let mut state = TouchGestureState::default();
        assert_eq!(
            state.update(&[
                touch(2, TouchPhase::Started, 0.0, 0.0),
                touch(1, TouchPhase::Started, 10.0, 0.0),
            ]),
            None
        );

        assert_eq!(
            state.update(&[
                touch(1, TouchPhase::Moved, 20.0, 0.0),
                touch(2, TouchPhase::Moved, 0.0, 0.0),
            ]),
            Some(TouchGesture::Pinch {
                previous_center: ScreenCoord { x: 5.0, y: 0.0 },
                current_center: ScreenCoord { x: 10.0, y: 0.0 },
                zoom: 0.5,
            })
        );
    }
}
