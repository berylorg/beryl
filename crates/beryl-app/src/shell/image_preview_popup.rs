use gpui::{Bounds, Pixels, Point, px};

const IMAGE_PREVIEW_POPUP_WIDTH: f32 = 560.0;
const IMAGE_PREVIEW_POPUP_HEIGHT: f32 = 380.0;

pub(super) fn popup_width() -> Pixels {
    px(IMAGE_PREVIEW_POPUP_WIDTH)
}

pub(super) fn popup_height() -> Pixels {
    px(IMAGE_PREVIEW_POPUP_HEIGHT)
}

pub(super) fn should_dismiss_for_mouse_down(
    bounds: Option<Bounds<Pixels>>,
    position: Point<Pixels>,
) -> bool {
    bounds.map_or(true, |bounds| !bounds.contains(&position))
}
