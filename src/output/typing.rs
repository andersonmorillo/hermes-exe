use std::mem::size_of;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clipboard_win::{formats, set_clipboard};
use tracing::warn;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteStyle {
    Standard,
    Terminal,
}

pub struct TextTyper;

impl TextTyper {
    pub fn new() -> Self {
        Self
    }

    pub fn type_text(&self, text: &str, paste_style: PasteStyle) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        release_modifier_keys()?;
        thread::sleep(Duration::from_millis(8));

        let paste_result = match paste_style {
            PasteStyle::Standard => paste_from_clipboard(text),
            PasteStyle::Terminal => paste_terminal_from_clipboard(text),
        };
        if let Err(err) = paste_result {
            warn!("clipboard paste failed; falling back to unicode typing: {err:#}");
            type_unicode(text)?;
        }

        Ok(())
    }
}

fn type_unicode(text: &str) -> Result<()> {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
    for code_unit in text.encode_utf16() {
        inputs.push(unicode_key_input(code_unit, false));
        inputs.push(unicode_key_input(code_unit, true));
    }
    send_inputs(&inputs, "unicode key events")
}

fn paste_from_clipboard(text: &str) -> Result<()> {
    set_clipboard(formats::Unicode, text)
        .map_err(|err| anyhow!("failed to set clipboard text: {err:?}"))?;
    thread::sleep(Duration::from_millis(12));
    send_chord_paste(&[VK_CONTROL], "clipboard paste key events")
}

fn paste_terminal_from_clipboard(text: &str) -> Result<()> {
    set_clipboard(formats::Unicode, text)
        .map_err(|err| anyhow!("failed to set clipboard text: {err:?}"))?;
    thread::sleep(Duration::from_millis(12));
    send_chord_paste(
        &[VK_CONTROL, VK_SHIFT],
        "terminal clipboard paste key events",
    )
}

fn send_chord_paste(modifiers: &[VIRTUAL_KEY], label: &str) -> Result<()> {
    let v = VIRTUAL_KEY('V' as u16);
    let mut inputs = Vec::with_capacity(modifiers.len() * 2 + 2);
    for modifier in modifiers {
        inputs.push(virtual_key_input(*modifier, false));
    }
    inputs.push(virtual_key_input(v, false));
    inputs.push(virtual_key_input(v, true));
    for modifier in modifiers.iter().rev() {
        inputs.push(virtual_key_input(*modifier, true));
    }
    send_inputs(&inputs, label)
}

fn send_inputs(inputs: &[INPUT], label: &str) -> Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(anyhow!(
            "SendInput wrote only {sent} of {} {label}",
            inputs.len()
        ));
    }
    Ok(())
}

fn unicode_key_input(code_unit: u16, keyup: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if keyup {
        flags |= KEYEVENTF_KEYUP;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code_unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn release_modifier_keys() -> Result<()> {
    let modifiers = [
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ];

    let mut inputs = Vec::with_capacity(modifiers.len());
    inputs.extend(modifiers.iter().map(|vk| virtual_key_input(*vk, true)));
    send_inputs(&inputs, "modifier key-up events")
}

fn virtual_key_input(vk: VIRTUAL_KEY, keyup: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if keyup {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
