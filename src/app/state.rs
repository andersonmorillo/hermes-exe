#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Idle,
    Recording,
    Transcribing,
    Typing,
    Error,
}

#[derive(Debug, Clone)]
pub struct AppState {
    phase: AppPhase,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            phase: AppPhase::Idle,
        }
    }

    pub fn phase(&self) -> AppPhase {
        self.phase
    }

    pub fn set_phase(&mut self, phase: AppPhase) {
        self.phase = phase;
    }
}
