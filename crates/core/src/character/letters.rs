//! A picture as `kelve-character/1` writes it: each layer 27 rows of 24
//! letters, `.` for none, and what colour each letter is. This is how the
//! person's agent goes to their page on kelve.ai, whatever it is: the
//! robot from its parts, one of kelve's characters, or one their agent
//! drew. A colour the person chose goes by its name, so the page follows
//! their colours as kelve does.

use std::collections::BTreeMap;

use super::{HEIGHT, Layer, Picture, WIDTH};

/// The most colours a character takes to the page.
pub const MOST_COLOURS: usize = 10;

/// A picture in letters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Letters {
    /// What colour each letter is: `#rrggbb`, or `body`, `light`,
    /// `screen` or `eyes`.
    pub colours: BTreeMap<String, String>,
    /// Each layer that has anything in it, and the ones every character
    /// needs, by name.
    pub layers: BTreeMap<String, Vec<String>>,
}

impl Picture {
    /// The picture in letters, `a` for the colour used most, `b` the next.
    /// Past [`MOST_COLOURS`], the rarest are drawn in the last colour kept:
    /// a few pixels in a near shade, rather than none.
    pub fn letters(&self) -> Letters {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        for layer in &self.layers {
            for paint in layer.iter().flatten() {
                *used.entry(String::from(paint.colour.clone())).or_default() += 1;
            }
        }
        let mut order: Vec<(String, usize)> = used.into_iter().collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let kept = order.len().min(MOST_COLOURS);
        let letter = |colour: &str| -> char {
            let at = order
                .iter()
                .position(|(c, _)| c == colour)
                .unwrap_or(0)
                .min(kept.saturating_sub(1));
            char::from(b'a' + at as u8)
        };
        let colours = order
            .iter()
            .take(kept)
            .enumerate()
            .map(|(i, (c, _))| (char::from(b'a' + i as u8).to_string(), c.clone()))
            .collect();
        let mut layers = BTreeMap::new();
        for layer in Layer::ALL {
            if !self.has(layer) && !layer.needed() {
                continue;
            }
            let grid = &self.layers[layer.index()];
            let rows = (0..HEIGHT)
                .map(|y| {
                    (0..WIDTH)
                        .map(|x| match &grid[y * WIDTH + x] {
                            Some(paint) => letter(&String::from(paint.colour.clone())),
                            None => '.',
                        })
                        .collect()
                })
                .collect();
            layers.insert(layer.name().to_string(), rows);
        }
        Letters { colours, layers }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Look, Move};
    use super::*;

    #[test]
    fn kelves_robot_and_characters_go_in_letters_on_the_grid() {
        for character in ["robot", "cat", "ghost"] {
            let look = Look {
                character: character.into(),
                ..Look::default()
            };
            let letters = look
                .picture(&BTreeMap::new(), &BTreeMap::new(), Move::Idle)
                .letters();
            assert!(letters.colours.len() <= MOST_COLOURS, "{character}");
            for needed in ["base", "eyes", "shut"] {
                assert!(letters.layers.contains_key(needed), "{character} {needed}");
            }
            for rows in letters.layers.values() {
                assert_eq!(rows.len(), HEIGHT);
                for row in rows {
                    assert_eq!(row.chars().count(), WIDTH);
                    assert!(
                        row.chars()
                            .all(|c| c == '.' || letters.colours.contains_key(&c.to_string()))
                    );
                }
            }
            assert!(
                letters.layers["base"]
                    .iter()
                    .any(|r| r.contains(|c| c != '.')),
                "{character} has a body"
            );
        }
    }

    #[test]
    fn the_colours_the_person_chose_go_by_name() {
        let letters = Look::default()
            .picture(&BTreeMap::new(), &BTreeMap::new(), Move::Idle)
            .letters();
        assert!(
            letters.colours.values().any(|c| c == "body"),
            "{:?}",
            letters.colours
        );
    }
}
