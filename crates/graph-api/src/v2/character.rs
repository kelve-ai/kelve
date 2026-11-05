//! A person's agent's character, as the service keeps it: `kelve-character/1`,
//! every layer drawn on the same grid, 24 pixels wide and 27 tall, one
//! letter a pixel and `.` for none, with what colour each letter is. A
//! robot built from kelve's parts is sent the same way, its parts drawn
//! into layers, so the service draws every character alike.
//!
//! TODO: once `kelve_core::character` lands, parse and check with it, and
//! keep only the API's shape here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The format every character names.
pub const FORMAT: &str = "kelve-character/1";

pub const WIDTH: usize = 24;
pub const HEIGHT: usize = 27;

/// The most colours a character may use.
pub const MOST_COLOURS: usize = 10;

/// The layers a character may have, in the order they're drawn, and how
/// kelve moves each: `base` bobs, `eyes` and `shut` blink, `light` glows,
/// the arms wave, and `legs` stays still while the rest bobs.
pub const LAYERS: [&str; 9] = [
    "legs",
    "base",
    "eyes",
    "shut",
    "light",
    "arm-left-down",
    "arm-left-up",
    "arm-right-down",
    "arm-right-up",
];

/// The layers every character has.
pub const NEEDED: [&str; 3] = ["base", "eyes", "shut"];

/// The colours a person chooses, which a character's letters may follow by
/// name: `body`, `light`, `screen` or `eyes`.
pub const ROLES: [&str; 4] = ["body", "light", "screen", "eyes"];

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    /// `kelve-character/1`.
    pub format: String,
    /// What it is, in a word or two: "Ghost". Never shown as its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What colour each letter is: `#rrggbb`, or one of the colours the
    /// person chose, by name.
    pub colours: BTreeMap<String, String>,
    /// The colours the person chose. kelve's own, without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<Palette>,
    /// Each layer, by name: 27 rows of 24.
    pub layers: BTreeMap<String, Vec<String>>,
}

/// The colours a person chose for their agent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub body: String,
    pub light: String,
    pub screen: String,
    pub eyes: String,
}

impl Default for Palette {
    /// kelve's robot's own.
    fn default() -> Self {
        Palette {
            body: "#8ecae6".into(),
            light: "#ff7a59".into(),
            screen: "#22304a".into(),
            eyes: "#9ff5d6".into(),
        }
    }
}

/// An `#rrggbb` colour's red, green and blue.
pub fn rgb(hex: &str) -> Option<[u8; 3]> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let at = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([at(0)?, at(2)?, at(4)?])
}

impl Character {
    /// What's wrong with it, in a sentence for the person's agent to fix,
    /// or nothing: pixels only, on the grid, in a few colours.
    pub fn check(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("A character is in the {FORMAT} format."));
        }
        if let Some(name) = &self.name
            && (name.chars().count() > 40 || name.chars().any(char::is_control))
        {
            return Err("A character’s name is a word or two, on one line.".into());
        }
        if self.colours.len() > MOST_COLOURS {
            return Err(format!("A character uses at most {MOST_COLOURS} colours."));
        }
        for (letter, colour) in &self.colours {
            let mut chars = letter.chars();
            if !matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_lowercase()) {
                return Err(format!(
                    "Each colour is for one lower-case letter, not “{}”.",
                    printable(letter)
                ));
            }
            if rgb(colour).is_none() && !ROLES.contains(&colour.as_str()) {
                return Err(format!(
                    "The colour for “{letter}” is #rrggbb, or body, light, screen or eyes."
                ));
            }
        }
        if let Some(p) = &self.palette
            && [&p.body, &p.light, &p.screen, &p.eyes]
                .iter()
                .any(|c| rgb(c).is_none())
        {
            return Err("The colours the person chose are each #rrggbb.".into());
        }
        for name in self.layers.keys() {
            if !LAYERS.contains(&name.as_str()) {
                return Err(format!(
                    "A character has no layer called “{}”. Its layers are {}.",
                    printable(name),
                    LAYERS.join(", ")
                ));
            }
        }
        for name in NEEDED {
            if !self.layers.contains_key(name) {
                return Err(format!("A character needs its {name} layer."));
            }
        }
        for (name, rows) in &self.layers {
            if rows.len() != HEIGHT || rows.iter().any(|r| r.chars().count() != WIDTH) {
                return Err(format!(
                    "The {name} layer is {HEIGHT} rows of {WIDTH} pixels."
                ));
            }
            for row in rows {
                for c in row.chars() {
                    if c != '.' && !self.colours.contains_key(c.encode_utf8(&mut [0; 4])) {
                        return Err(format!(
                            "The {name} layer uses “{}”, which has no colour. Use . for none.",
                            printable(&c.to_string())
                        ));
                    }
                }
            }
        }
        if self
            .layers
            .get("base")
            .is_some_and(|rows| rows.iter().all(|r| r.chars().all(|c| c == '.')))
        {
            return Err("The base layer is empty.".into());
        }
        Ok(())
    }

    /// The colour a letter is drawn in, the person's chosen colours
    /// followed: None for `.`, or a letter it has no colour for.
    pub fn colour(&self, letter: char) -> Option<[u8; 3]> {
        let named = self.colours.get(letter.encode_utf8(&mut [0; 4]))?;
        let palette = self.palette.clone().unwrap_or_default();
        let hex = match named.as_str() {
            "body" => &palette.body,
            "light" => &palette.light,
            "screen" => &palette.screen,
            "eyes" => &palette.eyes,
            hex => hex,
        };
        rgb(hex)
    }
}

/// Text from a character, made safe to say back: printable, and short.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(24).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    fn layer(pixels: &[(usize, usize, char)]) -> Vec<String> {
        let mut rows = vec![vec!['.'; WIDTH]; HEIGHT];
        for (x, y, c) in pixels {
            rows[*y][*x] = *c;
        }
        rows.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    fn ghost() -> Character {
        Character {
            format: FORMAT.into(),
            name: Some("Ghost".into()),
            colours: [("o", "#2b2f3a"), ("b", "body"), ("e", "eyes")]
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            palette: None,
            layers: [
                ("base", layer(&[(10, 10, 'o'), (11, 10, 'b')])),
                ("eyes", layer(&[(9, 8, 'e')])),
                ("shut", layer(&[(9, 9, 'e')])),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
        }
    }

    #[test]
    fn a_character_on_the_grid_in_its_colours_is_taken() {
        assert_eq!(ghost().check(), Ok(()));
        assert_eq!(ghost().colour('b'), Some([0x8e, 0xca, 0xe6]));
        assert_eq!(ghost().colour('o'), Some([0x2b, 0x2f, 0x3a]));
        assert_eq!(ghost().colour('.'), None);
    }

    #[test]
    fn what_is_wrong_is_said_so_the_agent_can_fix_it() {
        let mut c = ghost();
        c.format = "svg".into();
        assert!(c.check().is_err_and(|e| e.contains("kelve-character/1")));

        let mut c = ghost();
        c.layers.remove("shut");
        assert_eq!(c.check(), Err("A character needs its shut layer.".into()));

        let mut c = ghost();
        c.layers.insert("hat".into(), layer(&[]));
        assert!(
            c.check()
                .is_err_and(|e| e.contains("no layer called “hat”"))
        );

        let mut c = ghost();
        c.layers.get_mut("base").expect("base").pop();
        assert_eq!(
            c.check(),
            Err("The base layer is 27 rows of 24 pixels.".into())
        );

        let mut c = ghost();
        c.layers.insert("eyes".into(), layer(&[(1, 1, 'z')]));
        assert!(
            c.check()
                .is_err_and(|e| e.contains("“z”, which has no colour"))
        );

        let mut c = ghost();
        c.colours
            .insert("p".into(), "url(https://example.com)".into());
        assert!(c.check().is_err_and(|e| e.contains("#rrggbb")));

        let mut c = ghost();
        c.layers.insert("base".into(), layer(&[]));
        assert_eq!(c.check(), Err("The base layer is empty.".into()));
    }

    #[test]
    fn nothing_but_the_shape_is_taken() {
        let text = r##"{"format":"kelve-character/1","colours":{},"layers":{},"svg":"<svg/>"}"##;
        assert!(serde_json::from_str::<Character>(text).is_err());
    }
}
