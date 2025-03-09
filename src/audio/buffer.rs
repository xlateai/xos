use std::collections::VecDeque;

/// Buffer for a single audio channel
pub(crate) struct ChannelBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl ChannelBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub(crate) fn push(&mut self, sample: f32) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub(crate) fn push_slice(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.push(sample);
        }
    }

    pub(crate) fn get_samples(&self) -> Vec<f32> {
        self.samples.iter().copied().collect()
    }

    pub(crate) fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Buffer for multiple audio channels belonging to the same device
pub(crate) struct MultiChannelBuffer {
    channels: Vec<ChannelBuffer>,
}

impl MultiChannelBuffer {
    pub(crate) fn new(channel_count: usize, capacity_per_channel: usize) -> Self {
        let mut channels = Vec::with_capacity(channel_count);
        for _ in 0..channel_count {
            channels.push(ChannelBuffer::new(capacity_per_channel));
        }
        Self { channels }
    }

    pub(crate) fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub(crate) fn push_interleaved(&mut self, samples: &[f32]) {
        let channel_count = self.channels.len();
        let mut channel_index = 0;
        
        for &sample in samples {
            self.channels[channel_index].push(sample);
            channel_index = (channel_index + 1) % channel_count;
        }
    }

    pub(crate) fn push_planar(&mut self, samples: &[Vec<f32>]) {
        for (channel_index, channel_samples) in samples.iter().enumerate() {
            if channel_index < self.channels.len() {
                self.channels[channel_index].push_slice(channel_samples);
            }
        }
    }

    pub(crate) fn get_samples(&self) -> Vec<Vec<f32>> {
        self.channels.iter().map(|channel| channel.get_samples()).collect()
    }

    pub(crate) fn get_channel(&self, index: usize) -> Option<&ChannelBuffer> {
        self.channels.get(index)
    }

    pub(crate) fn get_channel_mut(&mut self, index: usize) -> Option<&mut ChannelBuffer> {
        self.channels.get_mut(index)
    }
}