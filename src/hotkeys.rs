//! User-configurable global shortcuts. Registered on the tray thread in `native`.

/// The global shortcut actions. Values are the `RegisterHotKey` ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Menu = 1,
    Pause = 2,
    Neural = 3,
    Compare = 4,
}
impl Action {
    pub const ALL: [Self; 4] = [Self::Neural, Self::Menu, Self::Compare, Self::Pause];
    pub fn id(self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Pause => "pause",
            Self::Neural => "neural",
            Self::Compare => "compare",
        }
    }
    pub fn describe(self) -> &'static str {
        match self {
            Self::Menu => "Show / hide controls",
            Self::Pause => "Clear / resume the overlay",
            Self::Neural => "Neural rendering on / off",
            Self::Compare => "Comparison controls",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Menu => "Menu",
            Self::Pause => "Clear",
            Self::Neural => "Neural",
            Self::Compare => "Compare",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows virtual-key code.
    pub vk: u32,
}
impl Hotkey {
    pub const fn key(vk: u32) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            vk,
        }
    }

    pub fn has_modifier(self) -> bool {
        self.ctrl || self.alt || self.shift
    }

    /// Keys that would block normal typing if registered on their own.
    pub fn needs_modifier(self) -> bool {
        !(0x70..=0x87).contains(&self.vk) // F1–F24
            && !matches!(self.vk, 0x13 | 0x2C | 0x91) // Pause, Print Screen, Scroll Lock
    }

    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(key_name(self.vk));
        parts.join("+")
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut hotkey = Self::key(0);
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => hotkey.ctrl = true,
                "alt" => hotkey.alt = true,
                "shift" => hotkey.shift = true,
                _ if hotkey.vk == 0 => hotkey.vk = key_code(part)?,
                _ => return None,
            }
        }
        (hotkey.vk != 0).then_some(hotkey)
    }

    /// A shortcut from an egui key press, if the key can be registered globally.
    pub fn from_egui(key: egui::Key, modifiers: egui::Modifiers) -> Option<Self> {
        Some(Self {
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
            vk: key_code(key.name())?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkeys {
    pub menu: Hotkey,
    pub pause: Hotkey,
    pub neural: Hotkey,
    pub compare: Hotkey,
}
impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            menu: Hotkey::key(0x77),    // F8
            neural: Hotkey::key(0x75),  // F6
            compare: Hotkey::key(0x78), // F9
            pause: Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                vk: 'O' as u32,
            },
        }
    }
}
impl Hotkeys {
    pub fn get(&self, action: Action) -> Hotkey {
        match action {
            Action::Menu => self.menu,
            Action::Pause => self.pause,
            Action::Neural => self.neural,
            Action::Compare => self.compare,
        }
    }

    pub fn set(&mut self, action: Action, hotkey: Hotkey) {
        match action {
            Action::Menu => self.menu = hotkey,
            Action::Pause => self.pause = hotkey,
            Action::Neural => self.neural = hotkey,
            Action::Compare => self.compare = hotkey,
        }
    }

    pub fn label(&self, action: Action) -> String {
        self.get(action).label()
    }

    /// The other action already using `hotkey`, if any.
    pub fn conflict(&self, action: Action, hotkey: Hotkey) -> Option<Action> {
        Action::ALL
            .into_iter()
            .find(|a| *a != action && self.get(*a) == hotkey)
    }
}

fn key_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap_or('?').to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x60..=0x69 => format!("Numpad{}", vk - 0x60),
        _ => NAMED
            .iter()
            .find(|(_, code)| *code == vk)
            .map_or_else(|| format!("Key{vk:02X}"), |(name, _)| name.to_string()),
    }
}

fn key_code(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(bytes[0] as u32);
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then_some(0x6F + n);
    }
    if let Some(n) = upper
        .strip_prefix("NUMPAD")
        .and_then(|n| n.parse::<u32>().ok())
    {
        return (n <= 9).then_some(0x60 + n);
    }
    if let Some(code) = upper
        .strip_prefix("KEY")
        .and_then(|n| u32::from_str_radix(n, 16).ok())
    {
        return Some(code);
    }
    NAMED
        .iter()
        .chain(ALIASES)
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, code)| *code)
}

/// Display names (and egui key names) for non-alphanumeric keys.
const NAMED: &[(&str, u32)] = &[
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Pause", 0x13),
    ("ScrollLock", 0x91),
    ("Minus", 0xBD),
    ("Equals", 0xBB),
    ("Comma", 0xBC),
    ("Period", 0xBE),
    ("Semicolon", 0xBA),
    ("Slash", 0xBF),
    ("Backtick", 0xC0),
    ("OpenBracket", 0xDB),
    ("Backslash", 0xDC),
    ("CloseBracket", 0xDD),
    ("Quote", 0xDE),
];
/// Alternative names egui uses for the same keys.
const ALIASES: &[(&str, u32)] = &[
    ("ArrowLeft", 0x25),
    ("ArrowUp", 0x26),
    ("ArrowRight", 0x27),
    ("ArrowDown", 0x28),
    ("Plus", 0xBB),
    ("Grave", 0xC0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_roundtrip() {
        let keys = Hotkeys::default();
        for action in Action::ALL {
            let hotkey = keys.get(action);
            assert_eq!(Hotkey::parse(&hotkey.label()), Some(hotkey));
        }
        assert_eq!(keys.label(Action::Pause), "Ctrl+Shift+O");
        assert_eq!(keys.label(Action::Menu), "F8");
        let custom = Hotkey::parse("alt+PageDown").unwrap();
        assert!(custom.alt && custom.vk == 0x22);
        assert_eq!(Hotkey::parse("Ctrl+Bogus"), None);
    }

    #[test]
    fn egui_keys_map_to_virtual_keys() {
        let none = egui::Modifiers::NONE;
        assert_eq!(
            Hotkey::from_egui(egui::Key::F7, none),
            Some(Hotkey::key(0x76))
        );
        assert_eq!(
            Hotkey::from_egui(egui::Key::K, none).map(|h| h.vk),
            Some(0x4B)
        );
        assert_eq!(
            Hotkey::from_egui(egui::Key::Num3, none).map(|h| h.vk),
            Some(0x33)
        );
        assert_eq!(
            Hotkey::from_egui(egui::Key::ArrowUp, none).map(|h| h.vk),
            Some(0x26)
        );
        assert!(Hotkey::key('K' as u32).needs_modifier());
        assert!(!Hotkey::key(0x76).needs_modifier());
    }

    #[test]
    fn detects_conflicts() {
        let keys = Hotkeys::default();
        assert_eq!(keys.conflict(Action::Neural, keys.menu), Some(Action::Menu));
        assert_eq!(keys.conflict(Action::Menu, keys.menu), None);
    }
}
