// src/random/native_random.rs

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::sync::Mutex;
use once_cell::sync::Lazy;

static RNG: Lazy<Mutex<StdRng>> = Lazy::new(|| {
    let seed = rand::random::<u64>();
    Mutex::new(StdRng::seed_from_u64(seed))
});

pub fn uniform() -> f64 {
    let mut rng = RNG.lock().unwrap();
    rng.gen()
}

pub fn uniform_range(min: f64, max: f64) -> f64 {
    let mut rng = RNG.lock().unwrap();
    rng.gen_range(min..max)
}

pub fn randint(min: i32, max: i32) -> i32 {
    let mut rng = RNG.lock().unwrap();
    rng.gen_range(min..max)
}
