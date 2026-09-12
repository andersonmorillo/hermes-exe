use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Result};
use rdev::{listen, EventType, Key};
use tracing::warn;

use crate::config::HotkeyConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyMode {
    F8Hold,
    CtrlShiftHold,
}

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    Pressed,
    Released,
    ListenerError(String),
}

pub struct HotkeyListener {
    rx: Receiver<HotkeyEvent>,
    _thread_handle: JoinHandle<()>,
}

impl HotkeyListener {
    pub fn start(config: HotkeyConfig) -> Result<Self> {
        let hotkey_mode = config.mode();
        if hotkey_mode.is_unknown() {
            warn!(
                "unsupported hotkey configuration {}+{}; using F8 hold",
                config.modifier, config.key
            );
        }

        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let thread_handle = thread::Builder::new()
            .name("hotkey-listener".to_string())
            .spawn(move || {
                let f8_down = Arc::new(AtomicBool::new(false));
                let ctrl_down = Arc::new(AtomicBool::new(false));
                let shift_down = Arc::new(AtomicBool::new(false));
                let ptt_down = Arc::new(AtomicBool::new(false));

                let f8_ref = Arc::clone(&f8_down);
                let ctrl_ref = Arc::clone(&ctrl_down);
                let shift_ref = Arc::clone(&shift_down);
                let ptt_ref = Arc::clone(&ptt_down);
                let tx_ref = tx.clone();
                let listen_result = listen(move |event| match event.event_type {
                    EventType::KeyPress(key) => {
                        if hotkey_mode.effective() == HotkeyMode::F8Hold && is_f8_key(key) {
                            let was_down = f8_ref.swap(true, Ordering::SeqCst);
                            if !was_down {
                                let was_pressed = ptt_ref.swap(true, Ordering::SeqCst);
                                if !was_pressed {
                                    let _ = tx_ref.send(HotkeyEvent::Pressed);
                                }
                            }
                            return;
                        }

                        if is_ctrl_key(key) {
                            ctrl_ref.store(true, Ordering::SeqCst);
                            if hotkey_mode.effective() == HotkeyMode::CtrlShiftHold
                                && shift_ref.load(Ordering::SeqCst)
                            {
                                let was_pressed = ptt_ref.swap(true, Ordering::SeqCst);
                                if !was_pressed {
                                    let _ = tx_ref.send(HotkeyEvent::Pressed);
                                }
                            }
                            return;
                        }

                        if is_shift_key(key) {
                            shift_ref.store(true, Ordering::SeqCst);
                            if hotkey_mode.effective() == HotkeyMode::CtrlShiftHold
                                && ctrl_ref.load(Ordering::SeqCst)
                            {
                                let was_pressed = ptt_ref.swap(true, Ordering::SeqCst);
                                if !was_pressed {
                                    let _ = tx_ref.send(HotkeyEvent::Pressed);
                                }
                            }
                        }
                    }
                    EventType::KeyRelease(key) => {
                        if hotkey_mode.effective() == HotkeyMode::F8Hold && is_f8_key(key) {
                            f8_ref.store(false, Ordering::SeqCst);
                            let was_pressed = ptt_ref.swap(false, Ordering::SeqCst);
                            if was_pressed {
                                let _ = tx_ref.send(HotkeyEvent::Released);
                            }
                            return;
                        }

                        if is_ctrl_key(key) {
                            ctrl_ref.store(false, Ordering::SeqCst);
                            let was_pressed = ptt_ref.swap(false, Ordering::SeqCst);
                            if was_pressed {
                                let _ = tx_ref.send(HotkeyEvent::Released);
                            }
                            return;
                        }

                        if is_shift_key(key) {
                            shift_ref.store(false, Ordering::SeqCst);
                            let was_pressed = ptt_ref.swap(false, Ordering::SeqCst);
                            if was_pressed {
                                let _ = tx_ref.send(HotkeyEvent::Released);
                            }
                        }
                    }
                    _ => {}
                });

                if let Err(error) = listen_result {
                    let _ = tx.send(HotkeyEvent::ListenerError(format!("{error:?}")));
                }
            })
            .map_err(|e| anyhow!("failed to spawn hotkey listener thread: {e}"))?;

        Ok(Self {
            rx,
            _thread_handle: thread_handle,
        })
    }

    pub fn try_recv(&self) -> Result<Option<HotkeyEvent>> {
        match self.rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(anyhow!("hotkey listener channel disconnected")),
        }
    }
}

impl HotkeyConfig {
    fn mode(&self) -> HotkeyModeConfig {
        let modifier_parts = self
            .modifier
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();
        let key_value = self.key.trim().to_ascii_lowercase();

        let no_modifier =
            modifier_parts.len() == 1 && matches_token(&modifier_parts[0], &["none", "", "null"]);
        if key_value == "f8" && no_modifier {
            return HotkeyModeConfig::Known(HotkeyMode::F8Hold);
        }

        let modifier_has_ctrl = modifier_parts.iter().any(|part| is_ctrl_token(part));
        let modifier_has_shift = modifier_parts.iter().any(|part| is_shift_token(part));
        let key_is_ctrl = is_ctrl_token(&key_value);
        let key_is_shift = is_shift_token(&key_value);

        if (modifier_has_ctrl && key_is_shift) || (modifier_has_shift && key_is_ctrl) {
            return HotkeyModeConfig::Known(HotkeyMode::CtrlShiftHold);
        }

        HotkeyModeConfig::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyModeConfig {
    Known(HotkeyMode),
    Unknown,
}

impl HotkeyModeConfig {
    fn effective(self) -> HotkeyMode {
        match self {
            HotkeyModeConfig::Known(mode) => mode,
            HotkeyModeConfig::Unknown => HotkeyMode::F8Hold,
        }
    }

    fn is_unknown(self) -> bool {
        matches!(self, HotkeyModeConfig::Unknown)
    }
}

fn is_ctrl_key(key: Key) -> bool {
    matches!(key, Key::ControlLeft | Key::ControlRight)
}

fn is_shift_key(key: Key) -> bool {
    matches!(key, Key::ShiftLeft | Key::ShiftRight)
}

fn is_ctrl_token(value: &str) -> bool {
    matches_token(
        value,
        &[
            "ctrl",
            "control",
            "lctrl",
            "rctrl",
            "leftctrl",
            "rightctrl",
            "left_control",
            "right_control",
            "control_left",
            "control_right",
        ],
    )
}

fn is_shift_token(value: &str) -> bool {
    matches_token(
        value,
        &[
            "shift",
            "lshift",
            "rshift",
            "leftshift",
            "rightshift",
            "left_shift",
            "right_shift",
            "shift_left",
            "shift_right",
        ],
    )
}

fn is_f8_key(key: Key) -> bool {
    key == Key::F8
}

fn matches_token(value: &str, options: &[&str]) -> bool {
    options.iter().any(|candidate| *candidate == value)
}
