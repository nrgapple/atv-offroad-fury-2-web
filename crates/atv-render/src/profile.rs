//! Optional per-pass CPU timing of a frame (off unless enabled; the parity
//! and CPU baselines run without it). Each `mark` charges the time since the
//! previous mark to a named slot; `report` gives the mean per frame.
use web_time::Instant;

#[derive(Default)]
pub struct Profiler {
    enabled: bool,
    last: Option<Instant>,
    slots: Vec<(&'static str, f64)>,
    counts: Vec<(&'static str, f64)>,
    frames: u32,
}

impl Profiler {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.slots.clear();
        self.counts.clear();
        self.frames = 0;
        self.last = None;
    }

    pub fn start(&mut self) {
        if self.enabled {
            self.last = Some(Instant::now());
        }
    }

    pub fn mark(&mut self, name: &'static str) {
        let Some(last) = self.last else {
            return;
        };
        let now = Instant::now();
        let ms = (now - last).as_secs_f64() * 1000.;
        self.last = Some(now);
        match self.slots.iter_mut().find(|(n, _)| *n == name) {
            Some((_, total)) => *total += ms,
            None => self.slots.push((name, ms)),
        }
    }

    /// Adds a per-frame count (draws of a pass) while profiling.
    pub fn count(&mut self, name: &'static str, n: u32) {
        if self.last.is_none() {
            return;
        }
        match self.counts.iter_mut().find(|(c, _)| *c == name) {
            Some((_, total)) => *total += f64::from(n),
            None => self.counts.push((name, f64::from(n))),
        }
    }

    pub fn end_frame(&mut self) {
        if self.last.take().is_some() {
            self.frames += 1;
        }
    }

    /// Mean milliseconds per profiled frame, in first-seen order.
    pub fn report(&self) -> Vec<(&'static str, f64)> {
        let n = f64::from(self.frames.max(1));
        self.slots
            .iter()
            .map(|(name, ms)| (*name, ms / n))
            .collect()
    }

    /// Mean counts per profiled frame.
    pub fn counts(&self) -> Vec<(&'static str, f64)> {
        let n = f64::from(self.frames.max(1));
        self.counts.iter().map(|(name, c)| (*name, c / n)).collect()
    }
}

/// Begin/end timestamps of the scene pass over a run of frames (native
/// profiling only). Frames are submitted back to back and read once at the
/// end, so the GPU stays busy instead of idling between waited frames.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period_ns: f32,
    frames: u32,
    /// Frame whose pass the next `writes` times; None leaves passes untimed.
    pub frame: Option<u32>,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl GpuTimer {
    pub fn new(gpu: &crate::Gpu, frames: u32) -> Self {
        let device = &gpu.device;
        let size = u64::from(frames) * 16;
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("scene timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: frames * 2,
            }),
            resolve: buffer(
                "scene timestamps",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            read: buffer(
                "scene timestamps read",
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            ),
            period_ns: gpu.queue.get_timestamp_period(),
            frames,
            frame: None,
        }
    }

    pub fn writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let frame = self.frame.filter(|&f| f < self.frames)?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(frame * 2),
            end_of_pass_write_index: Some(frame * 2 + 1),
        })
    }

    /// Waits for every submitted frame; returns each scene pass's GPU time.
    pub fn read_ms(&self, gpu: &crate::Gpu) -> Vec<f64> {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(&self.set, 0..self.frames * 2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, None);
        gpu.queue.submit([encoder.finish()]);
        self.read.map_async(wgpu::MapMode::Read, .., |_| {});
        if gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .is_err()
        {
            return Vec::new();
        }
        let ticks: Vec<u64> = match self.read.get_mapped_range(..) {
            Ok(data) => bytemuck::pod_collect_to_vec(&data[..]),
            Err(_) => Vec::new(),
        };
        self.read.unmap();
        ticks
            .as_chunks::<2>()
            .0
            .iter()
            .filter_map(|t| t[1].checked_sub(t[0]))
            .map(|t| t as f64 * f64::from(self.period_ns) / 1e6)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Profiler;

    #[test]
    fn charges_marks_only_while_enabled() {
        let mut p = Profiler::default();
        p.start();
        p.mark("a");
        p.count("draws", 3);
        p.end_frame();
        assert!(p.report().is_empty() && p.counts().is_empty());
        p.set_enabled(true);
        for _ in 0..2 {
            p.start();
            p.mark("a");
            p.mark("b");
            p.count("draws", 3);
            p.end_frame();
        }
        let names: Vec<_> = p.report().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(p.counts(), [("draws", 3.)]);
    }
}
