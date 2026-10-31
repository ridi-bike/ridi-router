use macroquad::prelude::*;
use macroquad::ui::{root_ui, widgets};

use crate::space::GpsCoord;
use crate::user_location::UserLocationState;

pub const BUTTON_MARGIN_PX: f32 = 48.0;
pub const BUTTON_WIDTH_PX: f32 = 224.0;
pub const BUTTON_HEIGHT_PX: f32 = 176.0;
pub const TAP_MAX_DISTANCE_PX: f32 = 12.0;

pub const UI_TEXT_SIZE: u16 = 32;

pub fn large_text_skin() -> macroquad::ui::Skin {
    let mut skin = root_ui().default_skin();

    skin.label_style = root_ui()
        .style_builder()
        .font_size(UI_TEXT_SIZE)
        .text_color(BLACK)
        .build();
    skin.button_style = root_ui()
        .style_builder()
        .font_size(UI_TEXT_SIZE)
        .text_color(BLACK)
        .text_color_hovered(BLACK)
        .text_color_clicked(BLACK)
        .color(Color::from_rgba(204, 204, 204, 235))
        .color_hovered(Color::from_rgba(170, 170, 170, 235))
        .color_clicked(Color::from_rgba(187, 187, 187, 255))
        .build();
    skin.window_titlebar_style = root_ui()
        .style_builder()
        .font_size(UI_TEXT_SIZE)
        .text_color(BLACK)
        .color(Color::from_rgba(68, 68, 68, 255))
        .color_inactive(Color::from_rgba(102, 102, 102, 127))
        .build();
    skin.tabbar_style = root_ui().style_builder().font_size(UI_TEXT_SIZE).build();
    skin.combobox_style = root_ui().style_builder().font_size(UI_TEXT_SIZE).build();
    skin.editbox_style = root_ui().style_builder().font_size(UI_TEXT_SIZE).build();
    skin.checkbox_style = root_ui().style_builder().font_size(UI_TEXT_SIZE).build();
    skin.title_height = 28.0;
    skin.margin = 4.0;

    skin
}

const RIGHT_DRAWER_ITEMS: [&str; 5] = [
    "Dummy entry 1",
    "Dummy entry 2",
    "Dummy entry 3",
    "Dummy entry 4",
    "Dummy entry 5",
];

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub right_drawer_open: bool,
    pub selected_right_item: Option<usize>,
    pub route_drawer_open: bool,
    pub route: RouteDrawerState,
    pub tap_chooser: Option<TapChooser>,
}

#[derive(Debug, Clone, Default)]
pub struct RouteDrawerState {
    pub start: Option<GpsCoord>,
    pub end: Option<GpsCoord>,
}

#[derive(Debug, Clone, Copy)]
pub struct TapChooser {
    pub coord: GpsCoord,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UiActions {
    pub locate_requested: bool,
    pub chooser_selection_consumed_input: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct UiLayout {
    pub locate_button: Rect,
    pub right_drawer_button: Rect,
    pub right_drawer: Rect,
    pub route_drawer_button: Rect,
    pub route_drawer: Rect,
    pub chooser_buttons: [Rect; 3],
}

impl UiState {
    pub fn layout(&self, screen_width: f32, screen_height: f32) -> UiLayout {
        let right_drawer = bottom_right_drawer(screen_width, screen_height);
        let route_drawer = bottom_left_drawer(screen_width, screen_height);

        UiLayout {
            locate_button: top_right_button(screen_width),
            right_drawer_button: bottom_right_button(
                screen_width,
                screen_height,
                BUTTON_MARGIN_PX
                    + if self.right_drawer_open {
                        right_drawer.w
                    } else {
                        0.0
                    },
                BUTTON_MARGIN_PX,
            ),
            right_drawer,
            route_drawer_button: bottom_left_button(
                screen_height,
                BUTTON_MARGIN_PX
                    + if self.route_drawer_open {
                        route_drawer.w
                    } else {
                        0.0
                    },
                BUTTON_MARGIN_PX,
            ),
            route_drawer,
            chooser_buttons: tap_chooser_buttons(screen_width, screen_height),
        }
    }

    pub fn draw(&mut self, user_location: UserLocationState) -> UiActions {
        let layout = self.layout(screen_width(), screen_height());
        let mut actions = UiActions::default();

        if icon_button(layout.locate_button, "LOC") {
            actions.locate_requested = true;
        }
        draw_button_caption(layout.locate_button, user_location.label());

        if icon_button(layout.right_drawer_button, "...") {
            self.right_drawer_open = !self.right_drawer_open;
        }
        if self.right_drawer_open {
            draw_right_drawer(self, layout.right_drawer);
        }

        if icon_button(layout.route_drawer_button, "-") {
            self.route_drawer_open = !self.route_drawer_open;
        }
        if self.route_drawer_open {
            draw_route_drawer(&self.route, layout.route_drawer);
        }

        if let Some(chooser) = self.tap_chooser {
            if draw_tap_chooser(chooser.coord, layout.chooser_buttons, &mut self.route) {
                self.tap_chooser = None;
                actions.chooser_selection_consumed_input = true;
            }
        }

        actions
    }
}

impl UiLayout {
    pub fn point_over_ui(&self, point: Vec2, state: &UiState) -> bool {
        self.locate_button.contains(point)
            || self.right_drawer_button.contains(point)
            || (state.right_drawer_open && self.right_drawer.contains(point))
            || self.route_drawer_button.contains(point)
            || (state.route_drawer_open && self.route_drawer.contains(point))
            || (state.tap_chooser.is_some()
                && self
                    .chooser_buttons
                    .iter()
                    .any(|button| button.contains(point)))
    }
}

fn top_right_button(screen_width: f32) -> Rect {
    Rect::new(
        screen_width - BUTTON_WIDTH_PX - BUTTON_MARGIN_PX,
        BUTTON_MARGIN_PX,
        BUTTON_WIDTH_PX,
        BUTTON_HEIGHT_PX,
    )
}

fn bottom_right_button(
    screen_width: f32,
    screen_height: f32,
    right_margin: f32,
    bottom_margin: f32,
) -> Rect {
    Rect::new(
        screen_width - BUTTON_WIDTH_PX - right_margin,
        screen_height - BUTTON_HEIGHT_PX - bottom_margin,
        BUTTON_WIDTH_PX,
        BUTTON_HEIGHT_PX,
    )
}

fn bottom_left_button(screen_height: f32, left_margin: f32, bottom_margin: f32) -> Rect {
    Rect::new(
        left_margin,
        screen_height - BUTTON_HEIGHT_PX - bottom_margin,
        BUTTON_WIDTH_PX,
        BUTTON_HEIGHT_PX,
    )
}

fn bottom_right_drawer(screen_width: f32, screen_height: f32) -> Rect {
    let width = screen_width / 3.0;
    let height = screen_height * 2.0 / 3.0;
    Rect::new(screen_width - width, screen_height - height, width, height)
}

fn bottom_left_drawer(screen_width: f32, screen_height: f32) -> Rect {
    let width = screen_width / 3.0;
    let height = screen_height * 2.0 / 3.0;
    Rect::new(0.0, screen_height - height, width, height)
}

fn tap_chooser_buttons(screen_width: f32, screen_height: f32) -> [Rect; 3] {
    let width = 180.0;
    let height = 88.0;
    let gap = 24.0;
    let total_width = width * 3.0 + gap * 2.0;
    let x = (screen_width - total_width) * 0.5;
    let y = (screen_height - height) * 0.5;
    [
        Rect::new(x, y, width, height),
        Rect::new(x + width + gap, y, width, height),
        Rect::new(x + (width + gap) * 2.0, y, width, height),
    ]
}

fn icon_button(rect: Rect, label: &str) -> bool {
    let clicked = widgets::Button::new(label)
        .position(vec2(rect.x, rect.y))
        .size(vec2(rect.w, rect.h))
        .ui(&mut root_ui());
    clicked || pointer_pressed_in(rect)
}

fn draw_button_caption(button: Rect, label: &str) {
    root_ui().label(
        Some(vec2(button.x + 12.0, button.y + button.h + 32.0)),
        label,
    );
}

fn pointer_pressed_in(rect: Rect) -> bool {
    let mouse_pressed = is_mouse_button_pressed(MouseButton::Left) && {
        let (x, y) = mouse_position();
        rect.contains(vec2(x, y))
    };
    mouse_pressed
        || touches()
            .iter()
            .any(|touch| touch.phase == TouchPhase::Started && rect.contains(touch.position))
}

fn draw_right_drawer(state: &mut UiState, drawer: Rect) {
    widgets::Window::new(
        macroquad::hash!("right_drawer"),
        vec2(drawer.x, drawer.y),
        vec2(drawer.w, drawer.h),
    )
    .label("Entries")
    .titlebar(false)
    .movable(false)
    .ui(&mut root_ui(), |ui| {
        ui.label(None, "Entries");
        ui.separator();
        for (index, label) in RIGHT_DRAWER_ITEMS.iter().enumerate() {
            let selected = state.selected_right_item == Some(index);
            let button_label = if selected {
                format!("* {label}")
            } else {
                (*label).to_owned()
            };
            let button_rect = Rect::new(
                drawer.x + 24.0,
                drawer.y + 58.0 + index as f32 * 64.0,
                (drawer.w - 48.0).max(1.0),
                64.0,
            );
            if widgets::Button::new(button_label.as_str())
                .size(vec2(button_rect.w, button_rect.h))
                .ui(ui)
                || pointer_pressed_in(button_rect)
            {
                state.selected_right_item = Some(index);
            }
        }
    });
}

fn draw_route_drawer(route: &RouteDrawerState, drawer: Rect) {
    widgets::Window::new(
        macroquad::hash!("route_drawer"),
        vec2(drawer.x, drawer.y),
        vec2(drawer.w, drawer.h),
    )
    .label("Route")
    .titlebar(false)
    .movable(false)
    .ui(&mut root_ui(), |ui| {
        ui.label(None, "Route");
        ui.separator();
        ui.label(None, "Start coordinates");
        ui.label(None, &coordinate_label(route.start));
        ui.separator();
        ui.label(None, "End coordinates");
        ui.label(None, &coordinate_label(route.end));
        ui.separator();
        ui.label(None, "Type");
        ui.label(None, "Default");
        ui.separator();
        let _ = widgets::Button::new("Generate")
            .size(vec2((drawer.w - 48.0).max(1.0), 64.0))
            .ui(ui);
    });
}

fn draw_tap_chooser(coord: GpsCoord, buttons: [Rect; 3], route: &mut RouteDrawerState) -> bool {
    let panel = Rect::new(
        buttons[0].x - 24.0,
        buttons[0].y - 72.0,
        buttons[2].x + buttons[2].w - buttons[0].x + 48.0,
        buttons[0].h + 104.0,
    );
    let mut selected = false;
    widgets::Window::new(
        macroquad::hash!("tap_chooser"),
        vec2(panel.x, panel.y),
        vec2(panel.w, panel.h),
    )
    .titlebar(false)
    .movable(false)
    .ui(&mut root_ui(), |ui| {
        ui.label(None, &format!("{:.6}, {:.6}", coord.lat, coord.lon));
        ui.separator();
        let start_rect = Rect::new(panel.x + 24.0, panel.y + 72.0, buttons[0].w, buttons[0].h);
        if widgets::Button::new("Start")
            .position(vec2(24.0, 72.0))
            .size(vec2(buttons[0].w, buttons[0].h))
            .ui(ui)
            || pointer_pressed_in(start_rect)
        {
            route.start = Some(coord);
            selected = true;
        }
        let end_rect = Rect::new(
            panel.x + 24.0 + buttons[0].w + 24.0,
            panel.y + 72.0,
            buttons[1].w,
            buttons[1].h,
        );
        if widgets::Button::new("End")
            .position(vec2(24.0 + buttons[0].w + 24.0, 72.0))
            .size(vec2(buttons[1].w, buttons[1].h))
            .ui(ui)
            || pointer_pressed_in(end_rect)
        {
            route.end = Some(coord);
            selected = true;
        }
        let cancel_rect = Rect::new(
            panel.x + 24.0 + (buttons[0].w + 24.0) * 2.0,
            panel.y + 72.0,
            buttons[2].w,
            buttons[2].h,
        );
        if widgets::Button::new("Cancel")
            .position(vec2(24.0 + (buttons[0].w + 24.0) * 2.0, 72.0))
            .size(vec2(buttons[2].w, buttons[2].h))
            .ui(ui)
            || pointer_pressed_in(cancel_rect)
        {
            selected = true;
        }
    });
    selected
}

fn coordinate_label(coord: Option<GpsCoord>) -> String {
    coord
        .map(|coord| format!("{:.6}, {:.6}", coord.lat, coord.lon))
        .unwrap_or_else(|| "Not selected".to_owned())
}
