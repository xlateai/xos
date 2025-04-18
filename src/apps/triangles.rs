use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::HashMap;

const TILE_SIZE: f64 = 1024.0;
const TILE_MARGIN: i32 = 1;
const POINTS_PER_TILE: usize = 64;

const LINE_COLOR: (u8, u8, u8) = (255, 255, 255);
const LINE_THICKNESS: i32 = 1;
const POINT_COLOR: (u8, u8, u8) = (214, 34, 64);
const POINT_RADIUS: i32 = 5;
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);

type TriangleKey = [usize; 3];

#[derive(Clone)]
struct IdentifiedPoint {
    id: usize,
    pos: Point,
}

struct Tile {
    origin: (i32, i32),
    points: Vec<IdentifiedPoint>,
    triangles: Vec<[usize; 3]>,
    colors: HashMap<TriangleKey, (u8, u8, u8)>,
}

pub struct TrianglesApp {
    scroll_x: f64,
    scroll_y: f64,
    dragging: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,
    tiles: HashMap<(i32, i32), Tile>,
    global_point_counter: usize,
}

impl TrianglesApp {
    pub fn new() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            tiles: HashMap::new(),
            global_point_counter: 0,
        }
    }

    fn get_tile(&mut self, tx: i32, ty: i32) -> &Tile {
        self.tiles.entry((tx, ty)).or_insert_with(|| {
            let seed = ((tx as u64) << 32) | (ty as u32 as u64);
            let mut rng = Pcg64::seed_from_u64(seed);
            let mut points = Vec::new();

            let base_id = self.global_point_counter;
            for i in 0..POINTS_PER_TILE {
                let x = rng.gen_range(0.0..TILE_SIZE);
                let y = rng.gen_range(0.0..TILE_SIZE);
                points.push(IdentifiedPoint {
                    id: base_id + i,
                    pos: Point { x, y },
                });
            }
            self.global_point_counter += POINTS_PER_TILE;

            let pos_points: Vec<Point> = points.iter().map(|p| p.pos.clone()).collect();
            let result = triangulate(&pos_points);

            let triangles: Vec<[usize; 3]> = result
                .triangles
                .chunks(3)
                .filter_map(|t| {
                    if t.len() == 3 {
                        Some([t[0], t[1], t[2]])
                    } else {
                        None
                    }
                })
                .collect();

            let mut colors = HashMap::new();
            for tri in &triangles {
                let mut ids = [
                    points[tri[0]].id,
                    points[tri[1]].id,
                    points[tri[2]].id,
                ];
                ids.sort();
                colors.insert(ids, random_purple(&mut rng));
            }

            Tile {
                origin: (tx, ty),
                points,
                triangles,
                colors,
            }
        })
    }
}

impl Application for TrianglesApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;
        let buffer = &mut state.frame.buffer;

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 255;
        }

        let screen_left = self.scroll_x;
        let screen_top = self.scroll_y;
        let screen_right = self.scroll_x + width as f64;
        let screen_bottom = self.scroll_y + height as f64;

        let tx_min = ((screen_left / TILE_SIZE).floor() as i32) - TILE_MARGIN;
        let tx_max = ((screen_right / TILE_SIZE).ceil() as i32) + TILE_MARGIN;
        let ty_min = ((screen_top / TILE_SIZE).floor() as i32) - TILE_MARGIN;
        let ty_max = ((screen_bottom / TILE_SIZE).ceil() as i32) + TILE_MARGIN;

        for ty in ty_min..=ty_max {
            for tx in tx_min..=tx_max {
                let scroll_x = self.scroll_x;
                let scroll_y = self.scroll_y;

                let tile = self.get_tile(tx, ty);
                let offset_x = tx as f64 * TILE_SIZE - scroll_x;
                let offset_y = ty as f64 * TILE_SIZE - scroll_y;

                let screen_points: Vec<Point> = tile
                    .points
                    .iter()
                    .map(|p| Point {
                        x: p.pos.x + offset_x,
                        y: p.pos.y + offset_y,
                    })
                    .collect();

                for tri in &tile.triangles {
                    let a = &screen_points[tri[0]];
                    let b = &screen_points[tri[1]];
                    let c = &screen_points[tri[2]];

                    let mut ids = [
                        tile.points[tri[0]].id,
                        tile.points[tri[1]].id,
                        tile.points[tri[2]].id,
                    ];
                    ids.sort();
                    let color = tile.colors[&ids];

                    let area = edge_function(a, b, c.x, c.y);
                    if area < 0.0 {
                        draw_filled_triangle(c, b, a, buffer, width as f64, height as f64, color);
                    } else {
                        draw_filled_triangle(a, b, c, buffer, width as f64, height as f64, color);
                    }

                    draw_line(a.x, a.y, b.x, b.y, buffer, width as f64, height as f64, LINE_COLOR);
                    draw_line(b.x, b.y, c.x, c.y, buffer, width as f64, height as f64, LINE_COLOR);
                    draw_line(c.x, c.y, a.x, a.y, buffer, width as f64, height as f64, LINE_COLOR);
                }

                for p in &screen_points {
                    draw_circle(p.x, p.y, POINT_RADIUS, buffer, width as f64, height as f64, POINT_COLOR);
                }
            }
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

fn random_purple<R: Rng>(rng: &mut R) -> (u8, u8, u8) {
    let r = rng.gen_range(100..180);
    let g = rng.gen_range(0..40);
    let b = rng.gen_range(180..255);
    (r, g, b)
}

// Drawing functions unchanged
fn draw_filled_triangle(a: &Point, b: &Point, c: &Point, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let min_x = a.x.min(b.x).min(c.x).floor() as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil() as i32;
    let min_y = a.y.min(b.y).min(c.y).floor() as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil() as i32;

    let area = edge_function(a, b, c.x, c.y);
    if area == 0.0 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let w0 = edge_function(b, c, px, py);
            let w1 = edge_function(c, a, px, py);
            let w2 = edge_function(a, b, px, py);

            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                put_pixel(x, y, buffer, width, height, color);
            }
        }
    }
}

fn edge_function(a: &Point, b: &Point, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

fn draw_line(x0: f64, y0: f64, x1: f64, y1: f64, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    if LINE_THICKNESS <= 1 {
        draw_thin_line(x0, y0, x1, y1, buffer, width, height, color);
    } else {
        for dx in -(LINE_THICKNESS / 2)..=(LINE_THICKNESS / 2) {
            for dy in -(LINE_THICKNESS / 2)..=(LINE_THICKNESS / 2) {
                draw_thin_line(x0 + dx as f64, y0 + dy as f64, x1 + dx as f64, y1 + dy as f64, buffer, width, height, color);
            }
        }
    }
}

fn draw_thin_line(x0: f64, y0: f64, x1: f64, y1: f64, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let (mut x0, mut y0, mut x1, mut y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;

    while x0 != x1 || y0 != y1 {
        put_pixel(x0, y0, buffer, width, height, color);
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(cx: f64, cy: f64, radius: i32, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let (cx, cy) = (cx as i32, cy as i32);
    let r2 = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                put_pixel(cx + dx, cy + dy, buffer, width, height, color);
            }
        }
    }
}

fn put_pixel(x: i32, y: i32, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let idx = (y as usize * width as usize + x as usize) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&[color.0, color.1, color.2, 255]);
    }
}
