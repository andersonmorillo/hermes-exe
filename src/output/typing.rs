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

pub struct TextTyper;

impl TextTyper {
    pub fn new() -> Self {
        Self
    }

    pub fn type_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        release_modifier_keys()?;
        thread::sleep(Duration::from_millis(8));

        if let Err(err) = paste_from_clipboard(text) {
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

    let ctrl = VK_CONTROL;
    let v = VIRTUAL_KEY('V' as u16);
    let inputs = [
        virtual_key_input(ctrl, false),
        virtual_key_input(v, false),
        virtual_key_input(v, true),
        virtual_key_input(ctrl, true),
    ];
    send_inputs(&inputs, "clipboard paste key events")
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
