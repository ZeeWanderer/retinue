#[derive(Default)]
pub(crate) enum FramePreparation {
    #[default]
    Idle,
    Waiting,
    Resuming,
}

impl FramePreparation {
    pub fn waiting(&self) -> bool {
        matches!(self, Self::Waiting)
    }
    pub fn ready(&self) -> bool {
        matches!(self, Self::Resuming)
    }
    pub fn can_sample(&self) -> bool {
        matches!(self, Self::Idle)
    }
    pub fn pending(&mut self) {
        *self = Self::Waiting;
    }
    pub fn wake(&mut self) {
        *self = Self::Resuming;
    }
    pub fn finish(&mut self) {
        *self = Self::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_resumes_the_same_sample_instead_of_restarting_its_animation() {
        let mut frame = FramePreparation::default();
        assert!(frame.can_sample());
        frame.pending();
        assert!(frame.waiting());
        assert!(!frame.can_sample());
        frame.wake();
        assert!(!frame.waiting());
        assert!(frame.ready());
        assert!(!frame.can_sample());
        frame.pending();
        frame.wake();
        frame.finish();
        assert!(frame.can_sample());
        assert!(!frame.ready());
    }

    #[test]
    fn work_started_by_input_can_wake_a_host_before_its_first_preparation_poll() {
        let mut frame = FramePreparation::default();
        frame.wake();
        frame.wake();
        assert!(frame.ready());
        assert!(!frame.can_sample());
        frame.finish();
        assert!(frame.can_sample());
    }
}
