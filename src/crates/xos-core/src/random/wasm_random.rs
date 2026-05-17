use js_sys::Math;

pub fn uniform() -> f64 {
    Math::random()
}

pub fn uniform_range(min: f64, max: f64) -> f64 {
    min + (max - min) * Math::random()
}

pub fn randint(min: i32, max: i32) -> i32 {
    let range = (max - min) as f64;
    (min as f64 + range * Math::random()).floor() as i32
}
