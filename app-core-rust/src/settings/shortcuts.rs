use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub(crate) modifiers: ShortcutModifiers,
    pub(crate) key: ShortcutKey,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShortcutModifiers {
    pub(crate) ctrl: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) win: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShortcutKey {
    Space,
    Letter(char),
    Digit(char),
    Function(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutParseError {
    message: &'static str,
}

impl ShortcutParseError {
    fn new(message: &'static str) -> Self {
        Self { message }
    }

    pub(crate) fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for ShortcutParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl Error for ShortcutParseError {}

impl Shortcut {
    pub(crate) fn parse(value: &str) -> Result<Self, ShortcutParseError> {
        let parts = value
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(ShortcutParseError::new("must include a modifier and key"));
        }

        let mut modifiers = ShortcutModifiers::default();
        let mut key = None;

        for part in parts {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => set_modifier(&mut modifiers.ctrl)?,
                "alt" => set_modifier(&mut modifiers.alt)?,
                "shift" => set_modifier(&mut modifiers.shift)?,
                "win" | "windows" | "meta" => set_modifier(&mut modifiers.win)?,
                _ => {
                    if key.is_some() {
                        return Err(ShortcutParseError::new("must contain only one key"));
                    }
                    key = Some(parse_key(part)?);
                }
            }
        }

        if !modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.win {
            return Err(ShortcutParseError::new("must include a modifier"));
        }

        Ok(Self {
            modifiers,
            key: key.ok_or_else(|| ShortcutParseError::new("must include a key"))?,
        })
    }

    pub(crate) fn conflicts_with(&self, other: &Self) -> bool {
        self == other
    }
}

fn set_modifier(modifier: &mut bool) -> Result<(), ShortcutParseError> {
    if *modifier {
        return Err(ShortcutParseError::new("must not repeat modifiers"));
    }

    *modifier = true;
    Ok(())
}

fn parse_key(value: &str) -> Result<ShortcutKey, ShortcutParseError> {
    let upper = value.to_ascii_uppercase();
    if upper == "SPACE" {
        return Ok(ShortcutKey::Space);
    }
    if upper.len() == 1 {
        let key = upper.chars().next().expect("one character");
        if key.is_ascii_alphabetic() {
            return Ok(ShortcutKey::Letter(key));
        }
        if key.is_ascii_digit() {
            return Ok(ShortcutKey::Digit(key));
        }
    }
    if let Some(number) = upper.strip_prefix('F') {
        if let Ok(number) = number.parse::<u8>() {
            if (1..=24).contains(&number) {
                return Ok(ShortcutKey::Function(number));
            }
        }
    }

    Err(ShortcutParseError::new("has an unsupported key"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(ctrl: bool, alt: bool, shift: bool, win: bool, key: ShortcutKey) -> Shortcut {
        Shortcut {
            modifiers: ShortcutModifiers {
                ctrl,
                alt,
                shift,
                win,
            },
            key,
        }
    }

    #[test]
    fn parse_accepts_supported_shortcuts() {
        let cases = [
            (
                "Ctrl+A",
                shortcut(true, false, false, false, ShortcutKey::Letter('A')),
            ),
            (
                "Ctrl+Alt+Space",
                shortcut(true, true, false, false, ShortcutKey::Space),
            ),
            (
                "Win+Shift+F12",
                shortcut(false, false, true, true, ShortcutKey::Function(12)),
            ),
            (
                "Control+A",
                shortcut(true, false, false, false, ShortcutKey::Letter('A')),
            ),
            (
                "Windows+A",
                shortcut(false, false, false, true, ShortcutKey::Letter('A')),
            ),
            (
                "Meta+A",
                shortcut(false, false, false, true, ShortcutKey::Letter('A')),
            ),
            (
                "Ctrl+Z",
                shortcut(true, false, false, false, ShortcutKey::Letter('Z')),
            ),
            (
                "Ctrl+7",
                shortcut(true, false, false, false, ShortcutKey::Digit('7')),
            ),
            (
                "Ctrl+F24",
                shortcut(true, false, false, false, ShortcutKey::Function(24)),
            ),
            (
                "A+Ctrl",
                shortcut(true, false, false, false, ShortcutKey::Letter('A')),
            ),
        ];

        for (value, expected) in cases {
            assert_eq!(Shortcut::parse(value), Ok(expected), "{value}");
        }
    }

    #[test]
    fn parse_rejects_invalid_shortcuts() {
        let cases = [
            ("Space", "must include a modifier and key"),
            ("A", "must include a modifier and key"),
            ("Ctrl+Ctrl+A", "must not repeat modifiers"),
            ("Ctrl+A+B", "must contain only one key"),
            ("Ctrl+Enter", "has an unsupported key"),
            ("Ctrl+Tab", "has an unsupported key"),
            ("", "must include a modifier and key"),
            ("+", "must include a modifier and key"),
            ("Ctrl+", "must include a modifier and key"),
            ("+A", "must include a modifier and key"),
            ("Ctrl+F0", "has an unsupported key"),
            ("Ctrl+F25", "has an unsupported key"),
        ];

        for (value, message) in cases {
            assert_eq!(
                Shortcut::parse(value),
                Err(ShortcutParseError::new(message)),
                "{value}"
            );
        }
    }

    #[test]
    fn conflicts_with_matches_equal_shortcuts_only() {
        let correct = Shortcut::parse("Ctrl+Alt+Space").expect("valid shortcut");
        let same = Shortcut::parse("Alt+Ctrl+Space").expect("valid shortcut");
        let different = Shortcut::parse("Ctrl+Shift+Space").expect("valid shortcut");

        assert!(correct.conflicts_with(&same));
        assert!(!correct.conflicts_with(&different));
    }
}
