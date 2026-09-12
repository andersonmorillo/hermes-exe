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
    RightCtrlRightShiftHold,
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
                let rctrl_down = Arc::new(AtomicBool::new(false));
                let rshift_down = Arc::new(AtomicBool::new(false));
                let ptt_down = Arc::new(AtomicBool::new(false));

                let f8_ref = Arc::clone(&f8_down);
                let rctrl_ref = Arc::clone(&rctrl_down);
                let rshift_ref = Arc::clone(&rshift_down);
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

                        if is_right_ctrl_key(key) {
                            rctrl_ref.store(true, Ordering::SeqCst);
                            if hotkey_mode.effective() == HotkeyMode::RightCtrlRightShiftHold
                                && rshift_ref.load(Ordering::SeqCst)
                            {
                                let was_pressed = ptt_ref.swap(true, Ordering::SeqCst);
                                if !was_pressed {
                                    let _ = tx_ref.send(HotkeyEvent::Pressed);
                                }
                            }
                            return;
                        }

                        if is_right_shift_key(key) {
                            rshift_ref.store(true, Ordering::SeqCst);
                            if hotkey_mode.effective() == HotkeyMode::RightCtrlRightShiftHold
                                && rctrl_ref.load(Ordering::SeqCst)
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

                        if is_right_ctrl_key(key) {
                            rctrl_ref.store(false, Ordering::SeqCst);
                            let was_pressed = ptt_ref.swap(false, Ordering::SeqCst);
                            if was_pressed {
                                let _ = tx_ref.send(HotkeyEvent::Released);
                            }
                            return;
                        }

                        if is_right_shift_key(key) {
                            rshift_ref.store(false, Ordering::SeqCst);
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

        let modifier_has_rctrl = modifier_parts.iter().any(|part| {
            matches_token(
                part,
                &["rctrl", "rightctrl", "right_control", "control_right"],
            )
        });
        let modifier_has_rshift = modifier_parts.iter().any(|part| {
            matches_token(
                part,
                &["rshift", "rightshift", "right_shift", "shift_right"],
            )
        });
        let key_is_rctrl = matches_token(
            &key_value,
            &["rctrl", "rightctrl", "right_control", "control_right"],
        );
        let key_is_rshift = matches_token(
            &key_value,
            &["rshift", "rightshift", "right_shift", "shift_right"],
        );

        if (modifier_has_rctrl && key_is_rshift) || (modifier_has_rshift && key_is_rctrl) {
            return HotkeyModeConfig::Known(HotkeyMode::RightCtrlRightShiftHold);
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

fn is_right_ctrl_key(key: Key) -> bool {
    key == Key::ControlRight
}

fn is_right_shift_key(key: Key) -> bool {
    key == Key::ShiftRight
}

fn is_f8_key(key: Key) -> bool {
    key == Key::F8
}

fn matches_token(value: &str, options: &[&str]) -> bool {
    options.iter().any(|candidate| *candidate == value)
}
