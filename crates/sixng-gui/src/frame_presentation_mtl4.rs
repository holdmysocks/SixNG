use metal_rust::metal::Device;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePhase {
    Real,
    Interpolated,
}
pub struct FramePresentationMtl4 {
    next_deadline: Instant,
    phase: FramePhase,
    real_presents: u64,
    interpolated_presents: u64,
}

impl FramePresentationMtl4 {
    pub fn new() -> Self {
        Self {
            next_deadline: Instant::now(),
            phase: FramePhase::Real,
            real_presents: 0,
            interpolated_presents: 0,
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

    pub fn advance(&mut self) {
        match self.phase {
            FramePhase::Real => {
                self.real_presents += 1;
                self.phase = FramePhase::Interpolated;
            }
            FramePhase::Interpolated => {
                self.interpolated_presents += 1;
                self.phase = FramePhase::Real;
            }
        }
    }

    pub fn pacing_interval(&self) -> Duration {
        if self.phase == FramePhase::Real {
            Duration::from_micros(4167)
        } else {
            Duration::from_micros(4166)
        }
    }

    pub fn phase(&self) -> FramePhase {
        self.phase
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.real_presents, self.interpolated_presents)
    }

    pub fn reset(&mut self) {
        self.phase = FramePhase::Real;
        self.next_deadline = Instant::now();
        self.real_presents = 0;
        self.interpolated_presents = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_alternates_real_and_interpolated() {
        let mut presentation = FramePresentationMtl4::new();
        assert_eq!(presentation.phase(), FramePhase::Real);
        presentation.advance();
        assert_eq!(presentation.phase(), FramePhase::Interpolated);
        presentation.advance();
        assert_eq!(presentation.phase(), FramePhase::Real);
        assert_eq!(presentation.counts(), (1, 1));
    }

    #[test]
    fn pacing_advances_without_deadline_drift() {
        let mut presentation = FramePresentationMtl4::new();
        let interval = Duration::from_micros(8_333);
        let first = presentation.schedule(interval);
        let second = presentation.schedule(interval);
        assert!(second >= first + interval);
    }
}
