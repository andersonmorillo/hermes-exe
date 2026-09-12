use std::thread;
use std::time::Duration;

use anyhow::Result;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tracing::warn;
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::HWND;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetClassNameW, GetForegroundWindow, GetWindowTextW, MessageBoxW, PeekMessageW,
    SetForegroundWindow, TranslateMessage, MB_ICONERROR, MB_OK, MSG, PM_REMOVE,
};

mod settings_dialog;

pub use settings_dialog::open_settings_dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Recording,
    Transcribing,
    Typing,
    Error,
}

pub struct WindowsTray {
    _tray: TrayIcon,
    settings_item_id: MenuId,
    quit_item_id: MenuId,
    open_settings_requested: bool,
    should_quit: bool,
}

impl WindowsTray {
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        let settings_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&settings_item)?;
        menu.append(&quit_item)?;

        let icon = blank_icon()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("Hermes: Idle")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .build()?;

        Ok(Self {
            _tray: tray,
            settings_item_id: settings_item.id().clone(),
            quit_item_id: quit_item.id().clone(),
            open_settings_requested: false,
            should_quit: false,
        })
    }

    pub fn tick(&mut self) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::DoubleClick { .. } => {
                    self.open_settings_requested = true;
                }
                _ => {}
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.settings_item_id {
                self.open_settings_requested = true;
            } else if event.id == self.quit_item_id {
                self.should_quit = true;
            }
        }
    }

    pub fn take_open_settings_requested(&mut self) -> bool {
        let requested = self.open_settings_requested;
        self.open_settings_requested = false;
        requested
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn set_status(&mut self, status: TrayStatus) {
        let label = match status {
            TrayStatus::Idle => "Hermes: Idle",
            TrayStatus::Recording => "Hermes: Recording",
            TrayStatus::Transcribing => "Hermes: Transcribing",
            TrayStatus::Typing => "Hermes: Typing",
            TrayStatus::Error => "Hermes: Error",
        };
        let _ = self._tray.set_tooltip(Some(label.to_string()));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ForegroundTarget {
    hwnd: HWND,
}

impl ForegroundTarget {
    pub fn restore(&self) {
        unsafe {
            if !SetForegroundWindow(self.hwnd).as_bool() {
                warn!("failed to restore focus to the selected window");
            }
        }
        thread::sleep(Duration::from_millis(35));
    }

    pub fn is_terminal(&self) -> bool {
        is_terminal_window(self.hwnd)
    }
}

pub fn is_terminal_window(hwnd: HWND) -> bool {
    window_class(hwnd).is_some_and(|class| is_terminal_window_class(&class))
        || window_title(hwnd).is_some_and(|title| is_terminal_window_title(&title))
}

fn window_class(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buffer);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(
            &buffer[..len.min(buffer.len() as i32) as usize],
        ))
    }
}

fn is_terminal_window_class(class: &str) -> bool {
    let class = class.to_ascii_lowercase();
    class.contains("console")
        || class.contains("cascadia")
        || class.contains("terminal")
        || class.contains("mintty")
}

fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(
            &buffer[..len.min(buffer.len() as i32) as usize],
        ))
    }
}

fn is_terminal_window_title(title: &str) -> bool {
    let title = title.to_ascii_lowercase();
    [
        "terminal",
        "powershell",
        "wsl",
        "cmd.exe",
        "command prompt",
        "bash",
        "zsh",
        "fish",
    ]
    .iter()
    .any(|hint| title.contains(hint))
}

pub fn capture_foreground_target(allow_terminal: bool) -> Option<ForegroundTarget> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let console = GetConsoleWindow();
        if !console.0.is_null() && hwnd == console {
            return None;
        }

        if !allow_terminal && is_terminal_window(hwnd) {
            return None;
        }

        Some(ForegroundTarget { hwnd })
    }
}

pub fn play_recording_start_beep() {
    play_system_sound(w!("SystemAsterisk"));
}

pub fn play_recording_stop_beep() {
    play_system_sound(w!("SystemDefault"));
}

fn play_system_sound(alias: windows::core::PCWSTR) {
    unsafe {
        let _ = PlaySoundW(alias, None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
    }
}

pub fn pump_message_queue() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn show_error_dialog(title: &str, message: &str) {
    let title = HSTRING::from(title);
    let message = HSTRING::from(message);
    unsafe {
        let _ = MessageBoxW(HWND::default(), &message, &title, MB_OK | MB_ICONERROR);
    }
}

fn blank_icon() -> Result<Icon> {
    let width = 16;
    let height = 16;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let on_border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
            if on_border {
                rgba.extend_from_slice(&[14_u8, 42_u8, 77_u8, 255_u8]);
            } else {
                rgba.extend_from_slice(&[0_u8, 160_u8, 220_u8, 255_u8]);
            }
        }
    }
    Ok(Icon::from_rgba(rgba, width, height)?)
}
