use metal_rust::metal::Device;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePhase {
    Interpolated,
    Real,
}

pub struct FramePresentationMtl4 {
    next_deadline: Instant,
    phase: FramePhase,
    real_presents: u64,
    interpolated_presents: u64,
    fallback_presents: u64,
    refresh_rate_millihertz: Option<u32>,
}

impl FramePresentationMtl4 {
    pub fn new(refresh_rate_millihertz: Option<u32>) -> Self {
        Self {
            next_deadline: Instant::now(),
            phase: FramePhase::Interpolated,
            real_presents: 0,
            interpolated_presents: 0,
            fallback_presents: 0,
            refresh_rate_millihertz,
        }
    }
    pub fn start(&mut self) -> anyhow::Result<()> {
        self.next_deadline = Instant::now();
        Ok(())
    }

    pub fn schedule(&mut self, interval: Duration) -> Instant {
        self.next_deadline += interval;
        let now = Instant::now();
        if self.next_deadline <= now {
            self.next_deadline = now + interval;
        }
        self.next_deadline
    }

    pub fn resize(&mut self, _device: &Device, size: PhysicalSize<u32>) -> anyhow::Result<()> {
        let width = size.width as usize;
        let height = size.height as usize;
        if width == 0 || height == 0 {
            return Ok(());
        }
        Ok(())
    }

    pub fn advance(&mut self, presented_interpolated: bool) {
        match self.phase {
            FramePhase::Interpolated if presented_interpolated => {
                self.interpolated_presents += 1;
                self.phase = FramePhase::Real;
            }
            FramePhase::Interpolated => {
                self.fallback_presents += 1;
                self.phase = FramePhase::Real;
            }
            FramePhase::Real => {
                self.real_presents += 1;
                self.phase = FramePhase::Interpolated;
            }
        }
    }

    pub fn pacing_interval(&self) -> Option<Duration> {
        self.refresh_rate_millihertz
            .filter(|rate| *rate >= 119_500)
            .map(|rate| Duration::from_nanos(1_000_000_000_000 / rate as u64))
    }

    pub fn set_refresh_rate_millihertz(&mut self, rate: Option<u32>) {
        self.refresh_rate_millihertz = rate;
    }

    pub fn supports_frame_generation(&self) -> bool {
        self.pacing_interval().is_some()
    }

    pub fn phase(&self) -> FramePhase {
        self.phase
    }

    pub fn should_render_real_frame(&self) -> bool {
        self.phase == FramePhase::Interpolated
    }

    pub fn counts(&self) -> (u64, u64, u64) {
        (
            self.real_presents,
            self.interpolated_presents,
            self.fallback_presents,
        )
    }

    pub fn reset(&mut self) {
        self.phase = FramePhase::Interpolated;
        self.next_deadline = Instant::now();
        self.real_presents = 0;
        self.interpolated_presents = 0;
        self.fallback_presents = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_alternates_real_and_interpolated() {
        let mut presentation = FramePresentationMtl4::new(Some(120_000));
        assert_eq!(presentation.phase(), FramePhase::Interpolated);
        presentation.advance(true);
        assert_eq!(presentation.phase(), FramePhase::Real);
        presentation.advance(false);
        assert_eq!(presentation.phase(), FramePhase::Interpolated);
        assert_eq!(presentation.counts(), (1, 1, 0));
    }

    #[test]
    fn pacing_advances_without_deadline_drift() {
        let mut presentation = FramePresentationMtl4::new(Some(120_000));
        let interval = Duration::from_micros(8_333);
        let first = presentation.schedule(interval);
        let second = presentation.schedule(interval);
        assert!(second >= first + interval);
    }

    #[test]
    fn fallback_is_not_counted_as_interpolation() {
        let mut presentation = FramePresentationMtl4::new(Some(120_000));
        presentation.advance(false);
        assert_eq!(presentation.counts(), (0, 0, 1));
    }

    #[test]
    fn frame_generation_requires_120hz_display() {
        assert!(!FramePresentationMtl4::new(Some(60_000)).supports_frame_generation());
        assert!(FramePresentationMtl4::new(Some(120_000)).supports_frame_generation());
    }
}
