//! Interactive poster text-layer layout shared by preview and export.

use gpui::{App, Bounds, Context, Entity, IntoElement, Pixels, Point, canvas};

pub(crate) const POSTER_WIDTH: u32 = 1080;
pub(crate) const POSTER_HEIGHT: u32 = 1920;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PosterTextLayer {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) font_size: f32,
}

#[derive(Debug, Clone, Copy)]
enum Grip {
    Move,
    Resize,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    layer: usize,
    grip: Grip,
    anchor: (f32, f32),
    origin: PosterTextLayer,
}

pub(crate) struct PosterLayout {
    layers: [PosterTextLayer; 3],
    bounds: Option<Bounds<Pixels>>,
    drag: Option<Drag>,
}

impl Default for PosterLayout {
    fn default() -> Self {
        Self {
            layers: [
                PosterTextLayer {
                    x: 0.08,
                    y: 0.19,
                    font_size: 124.0,
                },
                PosterTextLayer {
                    x: 0.08,
                    y: 0.32,
                    font_size: 58.0,
                },
                PosterTextLayer {
                    x: 0.69,
                    y: 0.08,
                    font_size: 42.0,
                },
            ],
            bounds: None,
            drag: None,
        }
    }
}

impl PosterLayout {
    pub(crate) fn layers(&self) -> [PosterTextLayer; 3] {
        self.layers
    }

    pub(crate) fn begin_move(
        &mut self,
        layer: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.begin(layer, Grip::Move, position, cx);
    }

    pub(crate) fn begin_resize(
        &mut self,
        layer: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.begin(layer, Grip::Resize, position, cx);
    }

    fn begin(&mut self, layer: usize, grip: Grip, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds else { return };
        let Some(origin) = self.layers.get(layer).copied() else {
            return;
        };
        self.drag = Some(Drag {
            layer,
            grip,
            anchor: normalized(position, bounds),
            origin,
        });
        cx.notify();
    }

    pub(crate) fn on_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(bounds), Some(drag)) = (self.bounds, self.drag) else {
            return;
        };
        let current = normalized(position, bounds);
        let layer = &mut self.layers[drag.layer];
        match drag.grip {
            Grip::Move => {
                layer.x = (drag.origin.x + current.0 - drag.anchor.0).clamp(0.0, 0.94);
                layer.y = (drag.origin.y + current.1 - drag.anchor.1).clamp(0.0, 0.96);
            }
            Grip::Resize => {
                let delta = (current.1 - drag.anchor.1) * POSTER_HEIGHT as f32;
                layer.font_size = (drag.origin.font_size + delta).clamp(24.0, 260.0);
            }
        }
        cx.notify();
    }

    pub(crate) fn on_up(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }
}

fn normalized(position: Point<Pixels>, bounds: Bounds<Pixels>) -> (f32, f32) {
    (
        ((position.x - bounds.left()) / bounds.size.width).clamp(0.0, 1.0),
        ((position.y - bounds.top()) / bounds.size.height).clamp(0.0, 1.0),
    )
}

pub(crate) fn bounds_capture(state: Entity<PosterLayout>) -> impl IntoElement {
    canvas(
        move |bounds, _, cx: &mut App| {
            state.update(cx, |layout, _| layout.bounds = Some(bounds));
        },
        |_, _, _, _| {},
    )
}
