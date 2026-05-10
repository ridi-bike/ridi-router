use crate::space::ScreenCoord;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DragState {
    previous_position: Option<ScreenCoord>,
}

impl DragState {
    pub fn update(
        &mut self,
        is_dragging: bool,
        current_position: ScreenCoord,
    ) -> Option<ScreenCoord> {
        if is_dragging {
            let previous_position = self.previous_position;
            self.previous_position = Some(current_position);
            previous_position
        } else {
            self.previous_position = None;
            None
        }
    }
}
