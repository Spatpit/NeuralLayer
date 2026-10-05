//! App preferences that are not part of the neural or ReShade configuration.
//! Neural options keep their own `SpatpitOptiScaler.ini` (read by the replay
//! tools) and the visual preset keeps `SpatpitEffects.ini`.
use crate::hotkeys::{Action, Hotkey, Hotkeys};
use crate::native::Area;
use crate::theme::Mode;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE: &str = "SpatpitOverlay.ini";
const VERSION: u32 = 1;
pub const FRAME_LIMITS: [u32; 4] = [30, 60, 90, 120];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub frame_limit: u32,
    pub always_on_top: bool,
    pub area: Area,
    pub outline: bool,
    pub menu_size: [f32; 2],
    pub adjustments: bool,
    pub sharpness: f32,
    pub saturation: f32,
    pub contrast: f32,
    /// Executable names of recently captured sources, newest first.
    pub recent: Vec<String>,
    pub hotkeys: Hotkeys,
    /// The show/hide shortcut brings back the command bar instead of the full menu.
    pub prefer_bar: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Mode::Dark,
            frame_limit: 60,
            always_on_top: true,
            area: Area::Content,
            outline: false,
            menu_size: [
                crate::menu::DEFAULT_MENU_SIZE.x,
                crate::menu::DEFAULT_MENU_SIZE.y,
            ],
            adjustments: false,
            sharpness: 0.3,
            saturation: 1.0,
            contrast: 1.0,
            recent: Vec::new(),
            hotkeys: Hotkeys::default(),
            prefer_bar: false,
        }
    }
}

pub fn path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

impl Settings {
    pub fn load(folder: &Path) -> Self {
        Self::parse(&std::fs::read_to_string(path(folder)).unwrap_or_default())
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.trim().trim_start_matches('\u{feff}').split_once('=')
            else {
                continue;
            };
            let value = value.trim();
            let number = value.parse::<f32>().ok().filter(|v| v.is_finite());
            let flag = number.map(|v| v != 0.);
            match key.trim() {
                "theme" => s.theme = Mode::from_id(value).unwrap_or(s.theme),
                "frame_limit" => {
                    if let Some(v) = number.map(|v| v as u32) {
                        if FRAME_LIMITS.contains(&v) {
                            s.frame_limit = v;
                        }
                    }
                }
                "always_on_top" => s.always_on_top = flag.unwrap_or(s.always_on_top),
                "area" => {
                    s.area = match value {
                        "window" => Area::Window,
                        _ => Area::Content,
                    }
                }
                "outline" => s.outline = flag.unwrap_or(s.outline),
                "menu_width" => {
                    if let Some(v) = number {
                        s.menu_size[0] = v.clamp(340., 1180.);
                    }
                }
                "menu_height" => {
                    if let Some(v) = number {
                        s.menu_size[1] = v.clamp(300., 920.);
                    }
                }
                "adjustments" => s.adjustments = flag.unwrap_or(s.adjustments),
                "sharpness" => s.sharpness = number.map_or(s.sharpness, |v| v.clamp(0., 2.)),
                "saturation" => s.saturation = number.map_or(s.saturation, |v| v.clamp(0., 2.)),
                "contrast" => s.contrast = number.map_or(s.contrast, |v| v.clamp(0.5, 1.5)),
                "controls" => s.prefer_bar = value == "bar",
                key if key.starts_with("hotkey_") => {
                    let action = Action::ALL
                        .into_iter()
                        .find(|a| key == format!("hotkey_{}", a.id()));
                    if let (Some(action), Some(hotkey)) = (action, Hotkey::parse(value)) {
                        s.hotkeys.set(action, hotkey);
                    }
                }
                "recent" => {
                    s.recent = value
                        .split('|')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .take(5)
                        .map(String::from)
                        .collect()
                }
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "version={VERSION}\ntheme={}\nframe_limit={}\nalways_on_top={}\narea={}\noutline={}\nmenu_width={}\nmenu_height={}\nadjustments={}\nsharpness={}\nsaturation={}\ncontrast={}\nrecent={}\ncontrols={}\n{}",
            self.theme.id(),
            self.frame_limit,
            u8::from(self.always_on_top),
            match self.area {
                Area::Content => "content",
                Area::Window => "window",
            },
            u8::from(self.outline),
            self.menu_size[0].round(),
            self.menu_size[1].round(),
            u8::from(self.adjustments),
            self.sharpness,
            self.saturation,
            self.contrast,
            self.recent.join("|"),
            if self.prefer_bar { "bar" } else { "menu" },
            Action::ALL
                .iter()
                .map(|a| format!("hotkey_{}={}\n", a.id(), self.hotkeys.label(*a)))
                .collect::<String>(),
        )
    }

    pub fn remember(&mut self, executable: &str) {
        if executable.is_empty() {
            return;
        }
        self.recent.retain(|e| !e.eq_ignore_ascii_case(executable));
        self.recent.insert(0, executable.to_string());
        self.recent.truncate(5);
    }
}

/// Coalesces frequent changes (slider drags) into one write after the
/// pointer is released and the value has been stable briefly.
#[derive(Default)]
pub struct Debounce {
    since: Option<Instant>,
}
impl Debounce {
    pub fn touch(&mut self) {
        self.since = Some(Instant::now());
    }
    pub fn pending(&self) -> bool {
        self.since.is_some()
    }
    /// True once when the change should be written.
    pub fn due(&mut self, pointer_down: bool) -> bool {
        match self.since {
            Some(since) if !pointer_down && since.elapsed() >= Duration::from_millis(400) => {
                self.since = None;
                true
            }
            _ => false,
        }
    }
    pub fn take(&mut self) -> bool {
        self.since.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Settings {
            theme: Mode::Light,
            frame_limit: 120,
            always_on_top: false,
            area: Area::Window,
            outline: true,
            menu_size: [800., 600.],
            adjustments: true,
            sharpness: 0.75,
            saturation: 1.25,
            contrast: 0.9,
            recent: Vec::new(),
            hotkeys: Hotkeys {
                menu: Hotkey::parse("Ctrl+Alt+M").unwrap(),
                ..Hotkeys::default()
            },
            prefer_bar: true,
        };
        s.remember("game.exe");
        s.remember("other.exe");
        s.remember("GAME.exe");
        assert_eq!(s.recent, ["GAME.exe", "other.exe"]);
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn rejects_malformed_values() {
        let s = Settings::parse("frame_limit=75\nsharpness=NaN\nmenu_width=99999\ntheme=neon\n");
        assert_eq!(s.frame_limit, 60);
        assert_eq!(s.sharpness, 0.3);
        assert_eq!(s.menu_size[0], 1180.);
        assert_eq!(s.theme, Mode::Dark);
    }
}
