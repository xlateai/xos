use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
use delaunator::{triangulate, Point};
use std::collections::HashMap;

use crate::random::{randint, uniform_range};

const VIEW_MARGIN: f64 = 512.0;
const SPAWN_PADDING: f64 = 128.0;
const POINT_DENSITY: f64 = 0.00015;
const MAX_POINTS: usize = 5000;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);

pub fn random_color() -> (u8, u8, u8) {
    let r = randint(0, 256);
    let g = randint(0, 11);
    let b = randint(0, 256);
    (r as u8, g as u8, b as u8)
}

#[derive(Clone)]
struct IdentifiedPoint {
    id: usize,
    pos: Point,
}

type TriangleKey = [usize; 3];

pub struct TrianglesApp {
    points: Vec<IdentifiedPoint>,
    triangles: Vec<[usize; 3]>,
    triangle_colors: HashMap<TriangleKey, (u8, u8, u8)>,
    /// Skip `triangulate` when the point set is unchanged (e.g. idle).
    last_delaunay_input: Option<Vec<Point>>,
    scroll_x: f64,
    scroll_y: f64,
    dragging: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,
    next_point_id: usize,
}

impl TrianglesApp {
    pub fn new() -> Self {
        Self {
            points: Vec::with_capacity(MAX_POINTS),
            triangles: Vec::with_capacity(MAX_POINTS),
            triangle_colors: HashMap::with_capacity(MAX_POINTS),
            last_delaunay_input: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            next_point_id: 0,
        }
    }

    fn regenerate(&mut self, width: f64, height: f64) {
        let view_left = self.scroll_x - VIEW_MARGIN;
        let view_top = self.scroll_y - VIEW_MARGIN;
        let view_right = self.scroll_x + width + VIEW_MARGIN;
        let view_bottom = self.scroll_y + height + VIEW_MARGIN;

        self.points.retain(|p| {
            let x = p.pos.x;
            let y = p.pos.y;
            x >= view_left && x <= view_right && y >= view_top && y <= view_bottom
        });

        let target_area = (view_right - view_left) * (view_bottom - view_top);
        let target_points = (POINT_DENSITY * target_area) as usize;

        while self.points.len() < target_points && self.points.len() < MAX_POINTS {
            let x = uniform_range(view_left - SPAWN_PADDING, view_right + SPAWN_PADDING);
            let y = uniform_range(view_top - SPAWN_PADDING, view_bottom + SPAWN_PADDING);
            self.points.push(IdentifiedPoint {
                id: self.next_point_id,
                pos: Point { x, y },
            });
            self.next_point_id += 1;
        }

        let delaunay_points: Vec<Point> = self.points.iter().map(|p| p.pos.clone()).collect();
        if self.last_delaunay_input.as_ref() == Some(&delaunay_points) {
            return;
        }
        self.last_delaunay_input = Some(delaunay_points.clone());

        let result = triangulate(&delaunay_points);

        self.triangles.clear();
        self.triangles.extend(
            result
                .triangles
                .chunks(3)
                .filter_map(|tri| (tri.len() == 3).then_some([tri[0], tri[1], tri[2]])),
        );

        let mut new_colors = HashMap::with_capacity(self.triangles.len());
        for tri in &self.triangles {
            let mut ids = [
                self.points[tri[0]].id,
                self.points[tri[1]].id,
                self.points[tri[2]].id,
            ];
            ids.sort();
            let key = ids;
            let color = if let Some(existing) = self.triangle_colors.get(&key) {
                *existing
            } else {
                let c = random_color();
                new_colors.insert(key, c);
                c
            };
            new_colors.insert(key, color);
        }

        self.triangle_colors = new_colors;
    }
}

impl Application for TrianglesApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f64;
        let height = shape[0] as f64;

        fill(
            &mut state.frame,
            (
                BACKGROUND_COLOR.0,
                BACKGROUND_COLOR.1,
                BACKGROUND_COLOR.2,
                255,
            ),
        );

        self.regenerate(width, height);

        let screen_points: Vec<Point> = self
            .points
            .iter()
            .map(|p| Point {
                x: p.pos.x - self.scroll_x,
                y: p.pos.y - self.scroll_y,
            })
            .collect();

        let n_tri = self.triangles.len();
        let mut flat: Vec<(f32, f32)> = Vec::with_capacity(n_tri.saturating_mul(3));
        let mut tri_colors: Vec<[u8; 4]> = Vec::with_capacity(n_tri);

        for tri in &self.triangles {
            let mut ids = [
                self.points[tri[0]].id,
                self.points[tri[1]].id,
                self.points[tri[2]].id,
            ];
            ids.sort();
            let color = self.triangle_colors[&ids];
            let rgba = [color.0, color.1, color.2, 255];

            let a = &screen_points[tri[0]];
            let b = &screen_points[tri[1]];
            let c = &screen_points[tri[2]];
            flat.push((a.x as f32, a.y as f32));
            flat.push((b.x as f32, b.y as f32));
            flat.push((c.x as f32, c.y as f32));
            tri_colors.push(rgba);
        }

        let _ = crate::rasterizer::triangles(&mut state.frame, &flat, &tri_colors);
    }

    fn on_scroll(&mut self, _state: &mut EngineState, dx: f32, dy: f32) {
        self.scroll_x += dx as f64;
        self.scroll_y += dy as f64;
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.dragging = true;
        self.last_mouse_x = state.mouse.x;
        self.last_mouse_y = state.mouse.y;
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let dx = state.mouse.x - self.last_mouse_x;
            let dy = state.mouse.y - self.last_mouse_y;
            self.scroll_x -= dx as f64;
            self.scroll_y -= dy as f64;
            self.last_mouse_x = state.mouse.x;
            self.last_mouse_y = state.mouse.y;
        }
    }
}
