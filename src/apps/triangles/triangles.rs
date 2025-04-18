use crate::engine::{Application, EngineState};
use crate::apps::triangles::geometric_utils::{
    draw_circle, draw_filled_triangle, draw_line, edge_function, random_gray,
};
use delaunator::{triangulate, Point};
use rand::Rng;
use std::collections::HashMap;

const VIEW_MARGIN: f64 = 512.0;
const SPAWN_PADDING: f64 = 128.0;
const POINT_DENSITY: f64 = 0.00015;
const MAX_POINTS: usize = 5000;

const LINE_COLOR: (u8, u8, u8) = (255, 255, 255);
const LINE_THICKNESS: i32 = 1;
const POINT_COLOR: (u8, u8, u8) = (214, 34, 64);
const POINT_RADIUS: i32 = 5;
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);

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
            scroll_x: 0.0,
            scroll_y: 0.0,
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            next_point_id: 0,
        }
    }

    fn regenerate(&mut self, width: f64, height: f64) {
        let mut rng = rand::thread_rng();
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
            let x = rng.gen_range(view_left - SPAWN_PADDING..view_right + SPAWN_PADDING);
            let y = rng.gen_range(view_top - SPAWN_PADDING..view_bottom + SPAWN_PADDING);
            self.points.push(IdentifiedPoint {
                id: self.next_point_id,
                pos: Point { x, y },
            });
            self.next_point_id += 1;
        }

        let delaunay_points: Vec<Point> = self.points.iter().map(|p| p.pos.clone()).collect();
        let result = triangulate(&delaunay_points);

        self.triangles.clear();
        self.triangles.extend(
            result.triangles.chunks(3).filter_map(|tri| {
                if tri.len() == 3 {
                    Some([tri[0], tri[1], tri[2]])
                } else {
                    None
                }
            }),
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
                let c = random_gray(&mut rng);
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
        let width = state.frame.width as f64;
        let height = state.frame.height as f64;
        let buffer = &mut state.frame.buffer;

        for chunk in buffer.chunks_exact_mut(4) {
            chunk[0] = BACKGROUND_COLOR.0;
            chunk[1] = BACKGROUND_COLOR.1;
            chunk[2] = BACKGROUND_COLOR.2;
            chunk[3] = 255;
        }

        self.regenerate(width, height);

        let screen_points: Vec<Point> = self
            .points
            .iter()
            .map(|p| Point {
                x: p.pos.x - self.scroll_x,
                y: p.pos.y - self.scroll_y,
            })
            .collect();

        for tri in &self.triangles {
            let a = &screen_points[tri[0]];
            let b = &screen_points[tri[1]];
            let c = &screen_points[tri[2]];

            let mut ids = [
                self.points[tri[0]].id,
                self.points[tri[1]].id,
                self.points[tri[2]].id,
            ];
            ids.sort();
            let color = self.triangle_colors[&ids];

            let area = edge_function(a, b, c.x, c.y);
            if area < 0.0 {
                draw_filled_triangle(c, b, a, buffer, width, height, color);
            } else {
                draw_filled_triangle(a, b, c, buffer, width, height, color);
            }

            draw_line(a.x, a.y, b.x, b.y, buffer, width, height, LINE_THICKNESS, LINE_COLOR);
            draw_line(b.x, b.y, c.x, c.y, buffer, width, height, LINE_THICKNESS, LINE_COLOR);
            draw_line(c.x, c.y, a.x, a.y, buffer, width, height, LINE_THICKNESS, LINE_COLOR);
        }

        for p in &screen_points {
            draw_circle(p.x, p.y, POINT_RADIUS, buffer, width, height, POINT_COLOR);
        }
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
