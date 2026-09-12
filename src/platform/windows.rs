use anyhow::Result;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MessageBoxW, PeekMessageW, TranslateMessage, MB_ICONERROR, MB_OK, MSG,
    PM_REMOVE,
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
