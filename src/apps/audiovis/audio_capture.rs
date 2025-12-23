#[cfg(not(target_arch = "wasm32"))]
use rodio::source::Source;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;

/// A Source wrapper that captures audio samples as they're played
#[cfg(not(target_arch = "wasm32"))]
pub struct SampleCapturingSource<S>
where
    S: Source,
{
    inner: S,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    max_buffer_size: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> SampleCapturingSource<S>
where
    S: Source,
{
    pub fn new(source: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>, max_buffer_size: usize) -> Self {
        Self {
            inner: source,
            sample_buffer,
            max_buffer_size,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> Iterator for SampleCapturingSource<S>
where
    S: Source,
{
    type Item = <S as Iterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.inner.next() {
            // Convert sample to f32 and capture it
            let sample_f32 = sample.to_f32();
            
            // Store in buffer
            let mut buffer = self.sample_buffer.lock().unwrap();
            buffer.push_back(sample_f32);
            
            // Keep buffer size reasonable
            while buffer.len() > self.max_buffer_size {
                buffer.pop_front();
            }
            
            Some(sample)
        } else {
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> Source for SampleCapturingSource<S>
where
    S: Source,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// Helper trait to convert samples to f32
#[cfg(not(target_arch = "wasm32"))]
trait ToF32 {
    fn to_f32(self) -> f32;
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}




