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
