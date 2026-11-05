//! The person's agent as a character: one pixel framework for every
//! character, so one looks the same in kelve, on a public page and in a
//! link's preview.
//!
//! Every character is drawn on the same grid, 24 pixels wide and 27 tall, in
//! layers that move the same way: `base` bobs, `eyes` and `shut` blink,
//! `light` glows, the arms wave, and `legs` stay still while the rest bobs.
//! kelve ships a robot made from parts, and two characters, in
//! `character/parts.json`: the one source the web app reads too. The
//! person's agent can draw a part for the robot (`kelve-part/1`) or a whole
//! character (`kelve-character/1`); kelve checks each before it shows it.
//!
//! The parts and characters are the canvas's Robot and Pixel boards, ported
//! pixel for pixel; `character/canvas.json` holds what those boards draw, and
//! the tests hold kelve to it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::redact::redact;

/// The grid every character is drawn on.
pub const WIDTH: usize = 24;
pub const HEIGHT: usize = 27;
/// What a part for the robot says it is.
pub const PART_FORMAT: &str = "kelve-part/1";
/// What a whole character says it is.
pub const CHARACTER_FORMAT: &str = "kelve-character/1";
/// The most colours in a part, and in a whole character.
pub const MOST_PART_COLOURS: usize = 6;
pub const MOST_CHARACTER_COLOURS: usize = 8;
/// The longest name for a part or a character, in characters.
pub const MOST_PART_NAME: usize = 40;
/// The longest name the person gives their agent.
pub const MOST_NAME: usize = 24;

/// The parts library, as the web app reads it too.
pub const PARTS: &str = include_str!("character/parts.json");

/// The four colours the person chooses, which parts can follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ColourRole")]
pub enum Role {
    Body,
    /// The light, and the hands: the accent.
    Light,
    Screen,
    Eyes,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Body, Role::Light, Role::Screen, Role::Eyes];

    pub fn name(self) -> &'static str {
        match self {
            Role::Body => "body",
            Role::Light => "light",
            Role::Screen => "screen",
            Role::Eyes => "eyes",
        }
    }
}

/// A colour in a drawing: one the person chose, by its role, or one that
/// stays the same, as `#rrggbb`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Colour {
    Role(Role),
    Hex(String),
}

impl Colour {
    /// A colour as a drawing writes it: `body`, `light`, `screen`, `eyes`,
    /// or `#rrggbb`.
    pub fn parse(text: &str) -> Option<Colour> {
        let t = text.trim();
        if let Some(role) = Role::ALL.into_iter().find(|r| r.name() == t) {
            return Some(Colour::Role(role));
        }
        hex(t).map(Colour::Hex)
    }
}

impl TryFrom<String> for Colour {
    type Error = String;

    fn try_from(text: String) -> Result<Colour, String> {
        Colour::parse(&text).ok_or_else(|| {
            format!("“{text}” isn't a colour: use body, light, screen, eyes or #rrggbb.")
        })
    }
}

impl From<Colour> for String {
    fn from(colour: Colour) -> String {
        match colour {
            Colour::Role(r) => r.name().to_string(),
            Colour::Hex(h) => h,
        }
    }
}

/// A colour as `#rrggbb`, lower case, if it is one.
fn hex(text: &str) -> Option<String> {
    let h = text.trim().strip_prefix('#')?;
    (h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", h.to_ascii_lowercase()))
}

/// The colours the person chose, each `#rrggbb`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CharacterColours")]
pub struct Colours {
    pub body: String,
    pub light: String,
    pub screen: String,
    pub eyes: String,
}

impl Colours {
    /// The colour a role has: the one chosen, if it's a colour, else the
    /// robot's own.
    pub fn of(&self, role: Role) -> String {
        let (chosen, own) = match role {
            Role::Body => (&self.body, &library().robot.colours.body),
            Role::Light => (&self.light, &library().robot.colours.light),
            Role::Screen => (&self.screen, &library().robot.colours.screen),
            Role::Eyes => (&self.eyes, &library().robot.colours.eyes),
        };
        hex(chosen).unwrap_or_else(|| own.clone())
    }
}

/// Where a part goes on the robot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PartSlot")]
pub enum Slot {
    Head,
    Antenna,
    Hat,
    Eyes,
    Mouth,
    Ears,
    Body,
    Chest,
    Hands,
    Legs,
}

impl Slot {
    pub const ALL: [Slot; 10] = [
        Slot::Head,
        Slot::Antenna,
        Slot::Hat,
        Slot::Eyes,
        Slot::Mouth,
        Slot::Ears,
        Slot::Body,
        Slot::Chest,
        Slot::Hands,
        Slot::Legs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Slot::Head => "head",
            Slot::Antenna => "antenna",
            Slot::Hat => "hat",
            Slot::Eyes => "eyes",
            Slot::Mouth => "mouth",
            Slot::Ears => "ears",
            Slot::Body => "body",
            Slot::Chest => "chest",
            Slot::Hands => "hands",
            Slot::Legs => "legs",
        }
    }

    pub fn of(name: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|s| s.name() == name.trim())
    }
}

/// A character's layers, each moved its own way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "CharacterLayer")]
pub enum Layer {
    /// Bobs while it waits.
    Base,
    /// Open, and shut, so it blinks.
    Eyes,
    Shut,
    /// Glows, faster while it works.
    Light,
    /// The arms, down and up, so it waves and cheers.
    ArmLeftDown,
    ArmLeftUp,
    ArmRightDown,
    ArmRightUp,
    /// Stays still while the rest bobs.
    Legs,
}

impl Layer {
    pub const ALL: [Layer; 9] = [
        Layer::Base,
        Layer::Eyes,
        Layer::Shut,
        Layer::Light,
        Layer::ArmLeftDown,
        Layer::ArmLeftUp,
        Layer::ArmRightDown,
        Layer::ArmRightUp,
        Layer::Legs,
    ];

    /// Its name in `kelve-character/1`.
    pub fn name(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Eyes => "eyes",
            Layer::Shut => "shut",
            Layer::Light => "light",
            Layer::ArmLeftDown => "arm-left-down",
            Layer::ArmLeftUp => "arm-left-up",
            Layer::ArmRightDown => "arm-right-down",
            Layer::ArmRightUp => "arm-right-up",
            Layer::Legs => "legs",
        }
    }

    pub fn of(name: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.name() == name.trim())
    }

    /// Whether every character must have it.
    pub fn needed(self) -> bool {
        matches!(self, Layer::Base | Layer::Eyes | Layer::Shut)
    }

    /// The classes its group has in a drawing: its name, and the one the
    /// animation moves.
    fn classes(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Eyes => "eyes eo",
            Layer::Shut => "shut ec",
            Layer::Light => "light",
            Layer::ArmLeftDown => "arm-left-down al-d",
            Layer::ArmLeftUp => "arm-left-up al-u",
            Layer::ArmRightDown => "arm-right-down ar-d",
            Layer::ArmRightUp => "arm-right-up ar-u",
            Layer::Legs => "legs",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How a character moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CharacterMove")]
pub enum Move {
    /// Waiting: it bobs, blinks and glows.
    #[default]
    Idle,
    /// Working: its arms go and its light glows faster.
    Work,
    /// Pleased: it jumps, arms up.
    Happy,
    /// Hello: it waves.
    Wave,
    /// Asleep: eyes shut, slow, with a z.
    Sleep,
    /// Not moving: how a choice is shown, and a preview's picture.
    Still,
}

impl Move {
    pub fn name(self) -> &'static str {
        match self {
            Move::Idle => "idle",
            Move::Work => "work",
            Move::Happy => "happy",
            Move::Wave => "wave",
            Move::Sleep => "sleep",
            Move::Still => "still",
        }
    }
}

// ---------------------------------------------------------------- the library

/// A colour of kelve's own parts, by its letter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ink {
    pub name: String,
    pub colour: Colour,
    /// How see-through it is, from 0 to 1, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// Whether it flickers, as a flame does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flicker: bool,
}

/// The colours the person picks from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    pub body: Vec<String>,
    pub light: Vec<String>,
    pub screen: Vec<String>,
    pub eyes: Vec<String>,
    pub background: Vec<String>,
}

/// A move, as people read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveLabel {
    pub key: Move,
    pub label: String,
}

/// Rows of pixels, with where their top left one goes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placed {
    #[serde(default)]
    pub at: [i32; 2],
    #[serde(default)]
    pub rows: Vec<String>,
}

/// How the robot's arms hang from its body: x from the body's leftmost
/// pixel, y from the top; the right arm is the left one mirrored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arms {
    pub down: Placed,
    pub up: Placed,
    /// Where a hand's top left pixel goes, down and up.
    pub hands: HandsAt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandsAt {
    pub down: [i32; 2],
    pub up: [i32; 2],
}

/// One of kelve's parts for a slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartOption {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub at: [i32; 2],
    #[serde(default)]
    pub rows: Vec<String>,
    /// Eyes: the eyes shut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shut: Option<Placed>,
    /// An antenna: what glows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<Placed>,
    /// Hands: the left hand, down and up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub down: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub up: Vec<String>,
}

/// A slot, with the box its parts fit in and kelve's parts for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotOptions {
    pub slot: Slot,
    pub label: String,
    /// x, y, width and height.
    #[serde(rename = "box")]
    pub within: [i32; 4],
    pub notes: String,
    pub options: Vec<PartOption>,
}

/// The robot: its default look, its fixed pieces, and its parts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RobotLibrary {
    pub label: String,
    pub parts: RobotParts,
    pub colours: Colours,
    pub background: String,
    /// The eyes it has while it's pleased, whatever eyes it has.
    pub pleased_eyes: String,
    pub neck: Placed,
    pub arms: Arms,
    pub slots: Vec<SlotOptions>,
}

/// One of kelve's own characters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryCharacter {
    pub key: String,
    pub label: String,
    /// Its colours until the person chooses others.
    pub colours: Colours,
    pub character: Character,
}

/// Everything in `parts.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub grid: [usize; 2],
    pub palette: BTreeMap<char, Ink>,
    pub colours: Choices,
    pub moves: Vec<MoveLabel>,
    pub robot: RobotLibrary,
    pub characters: Vec<LibraryCharacter>,
}

impl Library {
    /// A slot, with its options.
    pub fn slot(&self, slot: Slot) -> Option<&SlotOptions> {
        self.robot.slots.iter().find(|s| s.slot == slot)
    }

    /// One of kelve's parts, by its slot and key.
    pub fn option(&self, slot: Slot, key: &str) -> Option<&PartOption> {
        self.slot(slot)?.options.iter().find(|o| o.key == key)
    }

    /// One of kelve's characters, by its key.
    pub fn character(&self, key: &str) -> Option<&LibraryCharacter> {
        self.characters.iter().find(|c| c.key == key)
    }
}

static LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    serde_json::from_str(PARTS).expect("kelve's own parts library reads")
});

/// kelve's parts library.
pub fn library() -> &'static Library {
    &LIBRARY
}

// ---------------------------------------------------------------- the robot

/// The robot's parts, each by its key: one of kelve's, or one the person's
/// agent drew.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RobotParts {
    pub head: String,
    pub antenna: String,
    pub hat: String,
    pub eyes: String,
    pub mouth: String,
    pub ears: String,
    pub body: String,
    pub chest: String,
    pub hands: String,
    pub legs: String,
}

impl RobotParts {
    /// The part in a slot.
    pub fn get(&self, slot: Slot) -> &str {
        match slot {
            Slot::Head => &self.head,
            Slot::Antenna => &self.antenna,
            Slot::Hat => &self.hat,
            Slot::Eyes => &self.eyes,
            Slot::Mouth => &self.mouth,
            Slot::Ears => &self.ears,
            Slot::Body => &self.body,
            Slot::Chest => &self.chest,
            Slot::Hands => &self.hands,
            Slot::Legs => &self.legs,
        }
    }

    /// Puts a part in a slot.
    pub fn set(&mut self, slot: Slot, key: &str) {
        let place = match slot {
            Slot::Head => &mut self.head,
            Slot::Antenna => &mut self.antenna,
            Slot::Hat => &mut self.hat,
            Slot::Eyes => &mut self.eyes,
            Slot::Mouth => &mut self.mouth,
            Slot::Ears => &mut self.ears,
            Slot::Body => &mut self.body,
            Slot::Chest => &mut self.chest,
            Slot::Hands => &mut self.hands,
            Slot::Legs => &mut self.legs,
        };
        *place = key.to_string();
    }
}

/// A robot: its parts and four colours.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Robot {
    pub parts: RobotParts,
    pub colours: Colours,
}

impl Default for Robot {
    fn default() -> Robot {
        Robot {
            parts: library().robot.parts.clone(),
            colours: library().robot.colours.clone(),
        }
    }
}

/// How the person's agent looks: the character it is, and the colours and
/// background chosen for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "AgentLook")]
pub struct Look {
    /// `robot`, one of kelve's characters (`cat`, `ghost`), or one the
    /// agent drew, by its key.
    pub character: String,
    /// The robot's parts, and the colours every character follows.
    pub robot: Robot,
    /// The colour behind it, `#rrggbb`.
    pub background: String,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            character: "robot".to_string(),
            robot: Robot::default(),
            background: library().robot.background.clone(),
        }
    }
}

/// What a pixel is painted with.
#[derive(Clone, Debug, PartialEq)]
struct Paint {
    colour: Colour,
    opacity: Option<f64>,
    flicker: bool,
}

impl Paint {
    fn plain(colour: Colour) -> Paint {
        Paint {
            colour,
            opacity: None,
            flicker: false,
        }
    }
}

impl From<&Ink> for Paint {
    fn from(ink: &Ink) -> Paint {
        Paint {
            colour: ink.colour.clone(),
            opacity: ink.opacity,
            flicker: ink.flicker,
        }
    }
}

/// A character's pixels, layer by layer, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    layers: Vec<Vec<Option<Paint>>>,
}

impl Default for Picture {
    fn default() -> Picture {
        Picture {
            layers: vec![vec![None; WIDTH * HEIGHT]; Layer::ALL.len()],
        }
    }
}

impl Picture {
    /// Paints rows of letters onto a layer, their top left pixel at `at`,
    /// mirrored left to right when asked. Letters `ink` knows nothing of, and
    /// pixels off the grid, are left out.
    fn put(
        &mut self,
        layer: Layer,
        at: [i32; 2],
        rows: &[String],
        ink: &dyn Fn(char) -> Option<Paint>,
        mirror: bool,
    ) {
        let grid = &mut self.layers[layer.index()];
        for (dy, row) in rows.iter().enumerate() {
            for (dx, letter) in row.chars().enumerate() {
                if letter == '.' {
                    continue;
                }
                let x = at[0] + dx as i32;
                let x = if mirror { WIDTH as i32 - 1 - x } else { x };
                let y = at[1] + dy as i32;
                if !(0..WIDTH as i32).contains(&x) || !(0..HEIGHT as i32).contains(&y) {
                    continue;
                }
                if let Some(paint) = ink(letter) {
                    grid[y as usize * WIDTH + x as usize] = Some(paint);
                }
            }
        }
    }

    /// Whether a layer has anything in it.
    pub fn has(&self, layer: Layer) -> bool {
        self.layers[layer.index()].iter().any(Option::is_some)
    }

    /// The leftmost pixel of a layer, if it has any.
    fn leftmost(&self, layer: Layer) -> Option<i32> {
        self.layers[layer.index()]
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_some())
            .map(|(i, _)| (i % WIDTH) as i32)
            .min()
    }

    /// The picture as an SVG, its layers in named groups (`base`, `eyes`,
    /// `shut`, `light`, `arm-left-down` and the other arms, `legs`) with the
    /// classes `CSS` moves, and the colours chosen in place of the roles.
    pub fn svg(&self, colours: &Colours, draw: &Draw) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"px a-{}\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" shape-rendering=\"crispEdges\" role=\"img\" aria-label=\"{}\">",
            draw.moving.name(),
            escape(&draw.label)
        );
        if draw.style {
            let _ = write!(out, "<style>{CSS}</style>");
        }
        out.push_str(
            "<path d=\"M6 26h12v1h-12z\" fill=\"#1d1c1a\" opacity=\".1\"/><g class=\"all\">",
        );
        out.push_str(&self.group(Layer::Legs, colours));
        out.push_str("<g class=\"up\">");
        for layer in [
            Layer::Base,
            Layer::Eyes,
            Layer::Shut,
            Layer::Light,
            Layer::ArmLeftDown,
            Layer::ArmLeftUp,
            Layer::ArmRightDown,
            Layer::ArmRightUp,
        ] {
            out.push_str(&self.group(layer, colours));
        }
        out.push_str("</g><path class=\"zz\" d=\"M19 1h3v1h-3zM21 2h1v1h-1zM20 3h1v1h-1zM19 4h3v1h-3z\" fill=\"#8a847b\"/></g></svg>");
        out
    }

    /// A layer's group: a path for each paint, in the order each is first
    /// met, row by row, each row's runs of it as rectangles.
    fn group(&self, layer: Layer, colours: &Colours) -> String {
        let grid = &self.layers[layer.index()];
        let mut paints: Vec<(&Paint, String)> = Vec::new();
        for y in 0..HEIGHT {
            let mut x = 0;
            while x < WIDTH {
                let Some(paint) = &grid[y * WIDTH + x] else {
                    x += 1;
                    continue;
                };
                let mut end = x;
                while end + 1 < WIDTH && grid[y * WIDTH + end + 1].as_ref() == Some(paint) {
                    end += 1;
                }
                let n = end - x + 1;
                let run = format!("M{x} {y}h{n}v1h-{n}z");
                match paints.iter_mut().find(|(p, _)| *p == paint) {
                    Some((_, d)) => d.push_str(&run),
                    None => paints.push((paint, run)),
                }
                x = end + 1;
            }
        }
        if paints.is_empty() {
            return String::new();
        }
        let mut out = format!("<g class=\"{}\">", layer.classes());
        for (paint, d) in paints {
            let fill = match &paint.colour {
                Colour::Role(role) => colours.of(*role),
                Colour::Hex(h) => h.clone(),
            };
            let class = if paint.flicker {
                " class=\"flame\""
            } else {
                ""
            };
            let opacity = paint
                .opacity
                .map(|o| format!(" opacity=\"{o}\""))
                .unwrap_or_default();
            let _ = write!(out, "<path{class} d=\"{d}\" fill=\"{fill}\"{opacity}/>");
        }
        out.push_str("</g>");
        out
    }
}

/// How to draw a picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    pub moving: Move,
    /// What it's called, for people who can't see it: its name.
    pub label: String,
    /// Whether the SVG carries `CSS` itself, for a page that hasn't got it.
    pub style: bool,
}

/// Text made safe inside an attribute.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// How every character moves, as the canvas has it. Everything stops for
/// anyone whose computer asks for less motion.
pub const CSS: &str = ".px *{transform-box:view-box}
.px .ec,.px .al-u,.px .ar-u,.px .zz{opacity:0}
.a-idle .up,.a-wave .up{animation:px-bob 1.6s steps(1) infinite}
.a-idle .light{animation:px-glow 2.4s steps(1) infinite}
.a-idle .eo,.a-wave .eo,.a-work .eo{animation:px-blink-o 4.2s steps(1) infinite}
.a-idle .ec,.a-wave .ec,.a-work .ec{animation:px-blink-c 4.2s steps(1) infinite}
.a-wave .ar-u{animation:px-a .8s steps(1) infinite}
.a-wave .ar-d{animation:px-b .8s steps(1) infinite}
.a-wave .light{animation:px-glow .8s steps(1) infinite}
.a-work .al-u{animation:px-a .5s steps(1) infinite}
.a-work .al-d{animation:px-b .5s steps(1) infinite}
.a-work .ar-u{animation:px-b .5s steps(1) infinite}
.a-work .ar-d{animation:px-a .5s steps(1) infinite}
.a-work .light{animation:px-glow .4s steps(1) infinite}
.a-happy .all{animation:px-jump .7s steps(1) infinite}
.a-happy .al-u,.a-happy .ar-u{opacity:1}
.a-happy .al-d,.a-happy .ar-d{opacity:0}
.a-happy .light{animation:px-glow .35s steps(1) infinite}
.a-sleep .up{animation:px-bob 3.2s steps(1) infinite}
.a-sleep .eo{opacity:0}
.a-sleep .ec{opacity:1}
.a-sleep .light{opacity:.3}
.a-sleep .zz{animation:px-zz 2.4s steps(4) infinite}
.px .flame{animation:px-glow .3s steps(1) infinite}
@keyframes px-bob{0%{transform:translateY(0)}50%{transform:translateY(1px)}}
@keyframes px-jump{0%{transform:translateY(0)}50%{transform:translateY(-2px)}}
@keyframes px-glow{0%{opacity:1}50%{opacity:.4}}
@keyframes px-blink-o{0%{opacity:1}93%{opacity:0}97%{opacity:1}}
@keyframes px-blink-c{0%{opacity:0}93%{opacity:1}97%{opacity:0}}
@keyframes px-a{0%{opacity:1}50%{opacity:0}}
@keyframes px-b{0%{opacity:0}50%{opacity:1}}
@keyframes px-zz{0%{opacity:0;transform:translate(0,2px)}25%{opacity:1}100%{opacity:1;transform:translate(2px,-2px)}}
@media (prefers-reduced-motion: reduce){.px *{animation:none !important}}";

/// A part in a slot: one of kelve's, or one the agent drew.
enum Source<'a> {
    Own(&'a PartOption),
    Made(&'a Part),
}

/// The painting for kelve's own letters.
fn palette(letter: char) -> Option<Paint> {
    library().palette.get(&letter).map(Paint::from)
}

impl Robot {
    /// The robot's picture. `made` holds the parts the agent drew, by their
    /// keys; a part found nowhere is the robot's own for that slot. While
    /// it's pleased its eyes are happy, whatever eyes it has.
    pub fn picture(&self, made: &BTreeMap<String, Part>, pleased: bool) -> Picture {
        let lib = library();
        let source = |slot: Slot, key: &str| {
            if let Some(own) = lib.option(slot, key) {
                return Some(Source::Own(own));
            }
            if let Some(part) = made.get(key).filter(|p| p.slot == slot) {
                return Some(Source::Made(part));
            }
            lib.option(slot, lib.robot.parts.get(slot)).map(Source::Own)
        };
        let mut picture = Picture::default();
        let none = |key: &str| key == "none";

        // Each slot, in the order the canvas draws them.
        if let Some(s) = source(Slot::Ears, &self.parts.ears) {
            for mirror in [false, true] {
                place(&mut picture, Layer::Base, &s, mirror);
            }
        }
        let hatted = !none(&self.parts.hat) && !self.parts.hat.is_empty();
        if !hatted && let Some(s) = source(Slot::Antenna, &self.parts.antenna) {
            place(&mut picture, Layer::Base, &s, false);
            match &s {
                Source::Own(o) => {
                    if let Some(light) = &o.light {
                        picture.put(Layer::Light, light.at, &light.rows, &palette, false);
                    }
                }
                Source::Made(p) => {
                    picture.put(Layer::Light, p.at, &p.light, &|c| p.ink(c), false);
                }
            }
        }
        if let Some(s) = source(Slot::Head, &self.parts.head) {
            place(&mut picture, Layer::Base, &s, false);
        }
        let eyes = if pleased {
            lib.robot.pleased_eyes.as_str()
        } else {
            self.parts.eyes.as_str()
        };
        if let Some(s) = source(Slot::Eyes, eyes) {
            place(&mut picture, Layer::Eyes, &s, false);
            match &s {
                Source::Own(o) => {
                    if let Some(shut) = &o.shut {
                        picture.put(Layer::Shut, shut.at, &shut.rows, &palette, false);
                    }
                }
                Source::Made(p) => picture.put(Layer::Shut, p.at, &p.shut, &|c| p.ink(c), false),
            }
        }
        for slot in [Slot::Mouth, Slot::Hat] {
            if let Some(s) = source(slot, self.parts.get(slot)) {
                place(&mut picture, Layer::Base, &s, false);
            }
        }
        picture.put(
            Layer::Base,
            lib.robot.neck.at,
            &lib.robot.neck.rows,
            &palette,
            false,
        );
        let mut body = Picture::default();
        if let Some(s) = source(Slot::Body, &self.parts.body) {
            place(&mut picture, Layer::Base, &s, false);
            place(&mut body, Layer::Base, &s, false);
        }
        if let Some(s) = source(Slot::Chest, &self.parts.chest) {
            place(&mut picture, Layer::Base, &s, false);
        }

        // The arms hang from the body's sides, and the hands from the arms.
        let left = body.leftmost(Layer::Base).unwrap_or(6);
        let arms = &lib.robot.arms;
        let hand = source(Slot::Hands, &self.parts.hands);
        for (mirror, down, up) in [
            (false, Layer::ArmLeftDown, Layer::ArmLeftUp),
            (true, Layer::ArmRightDown, Layer::ArmRightUp),
        ] {
            let at = |p: [i32; 2]| [left + p[0], p[1]];
            picture.put(down, at(arms.down.at), &arms.down.rows, &palette, mirror);
            picture.put(up, at(arms.up.at), &arms.up.rows, &palette, mirror);
            match &hand {
                Some(Source::Own(o)) => {
                    picture.put(down, at(arms.hands.down), &o.down, &palette, mirror);
                    picture.put(up, at(arms.hands.up), &o.up, &palette, mirror);
                }
                Some(Source::Made(p)) => {
                    let ink = |c| p.ink(c);
                    let raised: Vec<String> = p.pixels.iter().rev().cloned().collect();
                    picture.put(down, at(arms.hands.down), &p.pixels, &ink, mirror);
                    picture.put(up, at(arms.hands.up), &raised, &ink, mirror);
                }
                None => {}
            }
        }

        if let Some(s) = source(Slot::Legs, &self.parts.legs) {
            place(&mut picture, Layer::Legs, &s, false);
        }
        picture
    }
}

/// Puts a part's main rows on a layer.
fn place(picture: &mut Picture, layer: Layer, source: &Source<'_>, mirror: bool) {
    match source {
        Source::Own(o) => picture.put(layer, o.at, &o.rows, &palette, mirror),
        Source::Made(p) => picture.put(layer, p.at, &p.pixels, &|c| p.ink(c), mirror),
    }
}

impl Look {
    /// What the person's agent looks like now. `made` holds the parts and
    /// characters its agent drew, by their keys. A character found nowhere is
    /// the robot.
    pub fn picture(
        &self,
        parts: &BTreeMap<String, Part>,
        characters: &BTreeMap<String, Character>,
        moving: Move,
    ) -> Picture {
        if self.character != "robot" {
            if let Some(own) = library().character(&self.character) {
                return own.character.picture();
            }
            if let Some(made) = characters.get(&self.character) {
                return made.picture();
            }
        }
        self.robot.picture(parts, moving == Move::Happy)
    }
}

impl Look {
    /// What the person's agent looks like now, as an SVG: `Look::picture`
    /// drawn in the colours chosen, named `label`.
    pub fn svg(
        &self,
        parts: &BTreeMap<String, Part>,
        characters: &BTreeMap<String, Character>,
        moving: Move,
        label: &str,
        style: bool,
    ) -> String {
        self.picture(parts, characters, moving).svg(
            &self.robot.colours,
            &Draw {
                moving,
                label: label.to_string(),
                style,
            },
        )
    }

    /// What's wrong with a look the person chose, if anything: a character,
    /// or a part for a slot, that's neither kelve's nor one their agent
    /// drew, or a colour that isn't `#rrggbb`.
    pub fn problem(
        &self,
        parts: &BTreeMap<String, Part>,
        characters: &BTreeMap<String, Character>,
    ) -> Option<String> {
        let lib = library();
        let known = self.character == "robot"
            || lib.character(&self.character).is_some()
            || characters.contains_key(&self.character);
        if !known {
            return Some(
                "That character isn't one kelve has, or one your agent drew. Choose another."
                    .into(),
            );
        }
        for slot in Slot::ALL {
            let key = self.robot.parts.get(slot);
            let fits =
                lib.option(slot, key).is_some() || parts.get(key).is_some_and(|p| p.slot == slot);
            if !fits {
                return Some(format!(
                    "That isn't a part for the robot's {}. Choose another.",
                    slot.name()
                ));
            }
        }
        let c = &self.robot.colours;
        for colour in [&c.body, &c.light, &c.screen, &c.eyes, &self.background] {
            if hex(colour).is_none() {
                return Some(
                    "Each colour is written #rrggbb. Choose one of the colours shown.".into(),
                );
            }
        }
        None
    }
}

/// Something the person's agent drew: a part for the robot, or a whole
/// character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawing {
    Part(Part),
    Character(Character),
}

/// Reads what the agent drew, by the format it says it is, and checks it.
pub fn drawing(text: &str) -> Result<Drawing, Vec<String>> {
    let format = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("format").and_then(Value::as_str).map(str::to_string));
    match format.as_deref() {
        Some(CHARACTER_FORMAT) => character(text).map(Drawing::Character),
        Some(PART_FORMAT) => part(text).map(Drawing::Part),
        _ if serde_json::from_str::<Value>(text).is_err() => part(text).map(Drawing::Part),
        _ => Err(vec![format!(
            "The drawing's format is “{PART_FORMAT}” for a part, or “{CHARACTER_FORMAT}” for a whole character."
        )]),
    }
}

/// The name the person's agent has until they give it one: "Sam’s agent".
pub fn default_name(first_name: &str) -> String {
    let first = first_name.split_whitespace().next().unwrap_or_default();
    if first.is_empty() {
        "Your agent".to_string()
    } else {
        format!("{first}’s agent")
    }
}

/// What's wrong with words that will be shown as a name: empty, too long,
/// or holding a link, markup, a path or a secret.
fn words_problem(what: &str, text: &str, most: usize) -> Option<String> {
    let t = text.trim();
    let n = t.chars().count();
    if n == 0 {
        return Some(format!("{what} is empty. Give it a few words."));
    }
    if n > most {
        return Some(format!(
            "{what} is {n} characters long, and the most is {most}. Shorten it."
        ));
    }
    if t.contains("://") || t.to_lowercase().contains("www.") {
        return Some(format!("{what} has a link in it. Take it out."));
    }
    if t.contains(['<', '>', '[', ']', '`', '*', '\n']) {
        return Some(format!("{what} has markup in it. Use plain words."));
    }
    if crate::md::ABSOLUTE.is_match(t) {
        return Some(format!(
            "{what} has a path on someone's computer in it. Take it out."
        ));
    }
    if redact(t) != t {
        return Some(format!("{what} seems to hold a secret. Take it out."));
    }
    None
}

/// What's wrong with a name the person gives their agent, if anything.
pub fn name_problem(name: &str) -> Option<String> {
    words_problem("The name", name, MOST_NAME)
}

// ---------------------------------------------------------------- what agents draw

/// A part for the robot, drawn by the person's agent (`kelve-part/1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DrawnPart")]
pub struct Part {
    pub name: String,
    pub slot: Slot,
    /// Where its top left pixel goes.
    pub at: [i32; 2],
    /// A colour for each letter.
    #[ts(type = "Record<string, string>")]
    pub colours: BTreeMap<char, Colour>,
    pub pixels: Vec<String>,
    /// Eyes: the eyes shut, at the same place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shut: Vec<String>,
    /// An antenna: what glows, at the same place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub light: Vec<String>,
}

impl Part {
    fn ink(&self, letter: char) -> Option<Paint> {
        self.colours.get(&letter).cloned().map(Paint::plain)
    }
}

/// A whole character, drawn by the person's agent (`kelve-character/1`),
/// or one of kelve's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DrawnCharacter")]
pub struct Character {
    pub name: String,
    #[ts(type = "Record<string, string>")]
    pub colours: BTreeMap<char, Colour>,
    #[ts(type = "Record<string, Array<string>>")]
    pub layers: BTreeMap<Layer, Vec<String>>,
}

impl Character {
    /// Its picture.
    pub fn picture(&self) -> Picture {
        let mut picture = Picture::default();
        let ink = |c: char| self.colours.get(&c).cloned().map(Paint::plain);
        for (layer, rows) in &self.layers {
            picture.put(*layer, [0, 0], rows, &ink, false);
        }
        picture
    }
}

/// What a drawing gives, read field by field, with every problem found.
struct Reader<'a> {
    what: &'static str,
    object: &'a Map<String, Value>,
    problems: Vec<String>,
}

impl Reader<'_> {
    fn string(&mut self, key: &str) -> Option<String> {
        match self.object.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => {
                self.problems
                    .push(format!("`{key}` in the {} is text, in quotes.", self.what));
                None
            }
            None => {
                self.problems
                    .push(format!("The {} has no `{key}`. Add it.", self.what));
                None
            }
        }
    }

    /// Rows of pixels, if given: strings of letters.
    fn rows(&mut self, key: &str, value: Option<&Value>) -> Option<Vec<String>> {
        match value? {
            Value::Array(items) => items
                .iter()
                .map(|i| i.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
                .or_else(|| {
                    self.problems.push(format!(
                        "`{key}` in the {} is a list of rows, each in quotes, such as \"..aa..\".",
                        self.what
                    ));
                    None
                }),
            _ => {
                self.problems.push(format!(
                    "`{key}` in the {} is a list of rows, each in quotes, such as \"..aa..\".",
                    self.what
                ));
                None
            }
        }
    }

    /// The colours, a letter for each.
    fn colours(&mut self, most: usize) -> BTreeMap<char, Colour> {
        let mut out = BTreeMap::new();
        let Some(given) = self.object.get("colours") else {
            self.problems.push(format!(
                "The {} has no `colours`. Give a colour for each letter, such as {{ \"a\": \"light\", \"g\": \"#f4c84a\" }}.",
                self.what
            ));
            return out;
        };
        let Some(given) = given.as_object() else {
            self.problems.push(format!(
                "`colours` in the {} gives a colour for each letter, such as {{ \"a\": \"light\", \"g\": \"#f4c84a\" }}.",
                self.what
            ));
            return out;
        };
        for (key, value) in given {
            let mut chars = key.chars();
            let letter = match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_lowercase() => c,
                _ => {
                    self.problems.push(format!(
                        "The colour “{}” isn't one lowercase letter. Give each colour a letter from a to z.",
                        crate::text::clip(key, 20)
                    ));
                    continue;
                }
            };
            match value.as_str().and_then(Colour::parse) {
                Some(colour) => {
                    out.insert(letter, colour);
                }
                None => self.problems.push(format!(
                    "The colour for “{letter}” isn't one kelve knows. Use body, light, screen, eyes, or a colour as #rrggbb."
                )),
            }
        }
        if given.is_empty() {
            self.problems.push(format!(
                "The {} has no colours. Give at least one.",
                self.what
            ));
        }
        if given.len() > most {
            self.problems.push(format!(
                "The {} has {} colours, and the most is {most}. Use fewer.",
                self.what,
                given.len()
            ));
        }
        out
    }

    /// What's wrong with rows of pixels: letters with no colour, and rows
    /// past `width` or `height`.
    fn check_rows(
        &mut self,
        name: &str,
        rows: &[String],
        colours: &BTreeMap<char, Colour>,
        width: usize,
        height: usize,
    ) {
        if rows.len() > height {
            self.problems.push(format!(
                "{name} has {} rows, and the most is {height}.",
                rows.len()
            ));
        }
        for (i, row) in rows.iter().enumerate() {
            let n = row.chars().count();
            if n > width {
                self.problems.push(format!(
                    "Row {} of {name} is {n} pixels wide, and the most is {width}.",
                    i + 1
                ));
            }
            let unknown: Vec<char> = row
                .chars()
                .filter(|c| *c != '.' && !colours.contains_key(c))
                .collect();
            if let Some(c) = unknown.first() {
                self.problems.push(format!(
                    "Row {} of {name} has “{c}”, which has no colour. Use a letter from colours, or “.” for nothing.",
                    i + 1
                ));
            }
        }
        if !rows.iter().any(|r| r.chars().any(|c| c != '.')) {
            self.problems
                .push(format!("{name} has no pixels. Draw something."));
        }
    }

    /// Problems for keys the format hasn't got.
    fn only(&mut self, keys: &[&str], says: &str) {
        for key in self.object.keys() {
            if !keys.contains(&key.as_str()) {
                self.problems.push(format!(
                    "The {} has `{}`, which kelve doesn't know. {says}",
                    self.what,
                    crate::text::clip(key, 30)
                ));
            }
        }
    }
}

/// The JSON object a drawing is, or why it isn't one.
fn object(text: &str, what: &str) -> Result<Map<String, Value>, Vec<String>> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(o)) => Ok(o),
        Ok(_) => Err(vec![format!("The {what} is one JSON object, in braces.")]),
        Err(e) => Err(vec![format!(
            "The {what} isn't JSON kelve can read: {} at line {}, column {}.",
            match e.classify() {
                serde_json::error::Category::Eof => "it ends too soon",
                serde_json::error::Category::Syntax => "a mistake in its syntax",
                _ => "something unexpected",
            },
            e.line(),
            e.column()
        )]),
    }
}

/// Reads a part the agent drew for the robot, and checks it: pixels only,
/// inside its slot's box, in at most six colours, with no words or
/// pictures from anywhere else.
pub fn part(text: &str) -> Result<Part, Vec<String>> {
    let object = object(text, "part")?;
    let mut r = Reader {
        what: "part",
        object: &object,
        problems: Vec::new(),
    };
    r.only(
        &["format", "name", "slot", "at", "colours", "pixels", "shut", "light"],
        "A part has only format, name, slot, at, colours and pixels, and shut for eyes or light for an antenna.",
    );
    if let Some(format) = r.string("format")
        && format != PART_FORMAT
    {
        r.problems
            .push(format!("The part's format is “{PART_FORMAT}”."));
    }
    let name = r.string("name").unwrap_or_default();
    if let Some(p) = words_problem("The part's name", &name, MOST_PART_NAME) {
        r.problems.push(p);
    }
    let slot = r.string("slot").and_then(|s| {
        let found = Slot::of(&s);
        if found.is_none() {
            let names: Vec<&str> = Slot::ALL.iter().map(|s| s.name()).collect();
            r.problems.push(format!(
                "The slot “{}” isn't one of the robot's. Use one of: {}.",
                crate::text::clip(&s, 20),
                names.join(", ")
            ));
        }
        found
    });
    let at = match object
        .get("at")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
    {
        Some([x, y]) => match (x.as_u64(), y.as_u64()) {
            (Some(x), Some(y)) if x < WIDTH as u64 && y < HEIGHT as u64 => {
                Some([x as i32, y as i32])
            }
            _ => None,
        },
        _ => None,
    };
    if at.is_none() {
        r.problems.push("`at` is where the part's top left pixel goes, as two whole numbers on the grid, such as [9, 0].".into());
    }
    let colours = r.colours(MOST_PART_COLOURS);
    let pixels = match r.rows("pixels", object.get("pixels")) {
        Some(p) => p,
        None => {
            if !object.contains_key("pixels") {
                r.problems
                    .push("The part has no `pixels`. Add its rows of letters.".into());
            }
            Vec::new()
        }
    };
    let shut = r.rows("shut", object.get("shut")).unwrap_or_default();
    let light = r.rows("light", object.get("light")).unwrap_or_default();

    if let (Some(slot), Some(at)) = (slot, at) {
        let within = library()
            .slot(slot)
            .map_or([0, 0, WIDTH as i32, HEIGHT as i32], |s| s.within);
        let width = pixels.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
        let height = pixels.len() as i32;
        let (bx, by, bw, bh) = (within[0], within[1], within[2], within[3]);
        if at[0] < bx || at[1] < by || at[0] + width > bx + bw || at[1] + height > by + bh {
            r.problems.push(format!(
                "The part goes outside its slot's box. A part for the {} fits in {bw} by {bh} pixels, with its top left at {bx}, {by}.",
                slot.name()
            ));
        }
        let room = |n: i32| n.max(0) as usize;
        r.check_rows(
            "pixels",
            &pixels,
            &colours,
            room(bx + bw - at[0]),
            room(by + bh - at[1]),
        );
        match slot {
            Slot::Eyes => {
                if shut.is_empty() {
                    r.problems.push("Eyes need `shut` too: the eyes shut, at the same place, so the robot can blink.".into());
                } else {
                    r.check_rows(
                        "shut",
                        &shut,
                        &colours,
                        room(bx + bw - at[0]),
                        room(by + bh - at[1]),
                    );
                }
            }
            _ if !shut.is_empty() => r
                .problems
                .push("Only eyes have `shut`. Take it out.".into()),
            _ => {}
        }
        match slot {
            Slot::Antenna if !light.is_empty() => {
                r.check_rows(
                    "light",
                    &light,
                    &colours,
                    room(bx + bw - at[0]),
                    room(by + bh - at[1]),
                );
            }
            Slot::Antenna => {}
            _ if !light.is_empty() => r
                .problems
                .push("Only an antenna has `light`. Take it out.".into()),
            _ => {}
        }
    }

    match (r.problems.is_empty(), slot, at) {
        (true, Some(slot), Some(at)) => Ok(Part {
            name: name.trim().to_string(),
            slot,
            at,
            colours,
            pixels,
            shut,
            light,
        }),
        _ => Err(r.problems),
    }
}

/// Reads a whole character the agent drew, and checks it: every layer on
/// the 24 by 27 grid, `base`, `eyes` and `shut` drawn, in at most eight
/// colours, with nothing else.
pub fn character(text: &str) -> Result<Character, Vec<String>> {
    let object = object(text, "character")?;
    let mut r = Reader {
        what: "character",
        object: &object,
        problems: Vec::new(),
    };
    r.only(
        &["format", "name", "colours", "layers"],
        "A character has only format, name, colours and layers.",
    );
    if let Some(format) = r.string("format")
        && format != CHARACTER_FORMAT
    {
        r.problems
            .push(format!("The character's format is “{CHARACTER_FORMAT}”."));
    }
    let name = r.string("name").unwrap_or_default();
    if let Some(p) = words_problem("The character's name", &name, MOST_PART_NAME) {
        r.problems.push(p);
    }
    let colours = r.colours(MOST_CHARACTER_COLOURS);
    let mut layers = BTreeMap::new();
    match object.get("layers").and_then(Value::as_object) {
        None => r.problems.push(
            "The character has no `layers`. Give base, eyes and shut at least, each as rows of letters."
                .into(),
        ),
        Some(given) => {
            for (key, value) in given {
                let Some(layer) = Layer::of(key) else {
                    let names: Vec<&str> = Layer::ALL.iter().map(|l| l.name()).collect();
                    r.problems.push(format!(
                        "The layer “{}” isn't one kelve moves. Use: {}.",
                        crate::text::clip(key, 20),
                        names.join(", ")
                    ));
                    continue;
                };
                if let Some(rows) = r.rows(key, Some(value)) {
                    r.check_rows(&format!("The layer “{key}”"), &rows, &colours, WIDTH, HEIGHT);
                    layers.insert(layer, rows);
                }
            }
            for layer in Layer::ALL.into_iter().filter(|l| l.needed()) {
                if !given.contains_key(layer.name()) {
                    r.problems.push(format!(
                        "The character has no “{}” layer. Every character has base, eyes and shut.",
                        layer.name()
                    ));
                }
            }
        }
    }
    if r.problems.is_empty() {
        Ok(Character {
            name: name.trim().to_string(),
            colours,
            layers,
        })
    } else {
        Err(r.problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A picture's layer as rows of kelve's own letters, to compare.
    fn rows(picture: &Picture, layer: Layer) -> Vec<String> {
        let lib = library();
        let grid = &picture.layers[layer.index()];
        (0..HEIGHT)
            .map(|y| {
                (0..WIDTH)
                    .map(|x| match &grid[y * WIDTH + x] {
                        None => '.',
                        Some(p) => lib
                            .palette
                            .iter()
                            .find(|(_, ink)| Paint::from(*ink) == *p)
                            .map_or('?', |(c, _)| *c),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_library_reads_and_its_parts_fit_their_slots() {
        let lib = library();
        assert_eq!(lib.grid, [WIDTH, HEIGHT]);
        assert_eq!(lib.robot.slots.len(), Slot::ALL.len());
        for slot in Slot::ALL {
            let s = lib.slot(slot).unwrap();
            // The robot's own part for every slot is there.
            assert!(
                lib.option(slot, lib.robot.parts.get(slot)).is_some(),
                "{slot:?}"
            );
            let [bx, by, bw, bh] = s.within;
            for o in &s.options {
                for letter in o
                    .rows
                    .iter()
                    .chain(&o.down)
                    .chain(&o.up)
                    .flat_map(|r| r.chars())
                {
                    assert!(
                        letter == '.' || lib.palette.contains_key(&letter),
                        "{}: {letter}",
                        o.key
                    );
                }
                if slot == Slot::Hands || o.rows.is_empty() {
                    continue;
                }
                let w = o.rows.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
                let h = o.rows.len() as i32;
                assert!(
                    o.at[0] >= bx
                        && o.at[1] >= by
                        && o.at[0] + w <= bx + bw
                        && o.at[1] + h <= by + bh,
                    "{} doesn't fit the {slot:?} box",
                    o.key
                );
            }
        }
        assert!(lib.option(Slot::Eyes, &lib.robot.pleased_eyes).is_some());
        assert_eq!(lib.moves.len(), 5);
        assert_eq!(lib.colours.background.len(), 6);
        // kelve's own characters pass the checks an agent's do.
        assert_eq!(lib.characters.len(), 2);
        for c in &lib.characters {
            let text = serde_json::to_string(&json!({
                "format": CHARACTER_FORMAT,
                "name": c.character.name,
                "colours": c.character.colours,
                "layers": c.character.layers,
            }))
            .unwrap();
            assert_eq!(character(&text).unwrap(), c.character, "{}", c.key);
        }
    }

    #[test]
    fn robots_are_drawn_exactly_as_the_canvas_draws_them() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("character/canvas.json")).unwrap();
        assert!(cases.len() >= 4);
        for case in cases {
            let given = case["robot"].as_object().unwrap();
            let mut robot = Robot::default();
            for slot in Slot::ALL {
                let prop = if slot == Slot::Hands {
                    "arms"
                } else {
                    slot.name()
                };
                if let Some(key) = given.get(prop).and_then(Value::as_str) {
                    robot.parts.set(slot, key);
                }
            }
            let pleased = given.get("anim").and_then(Value::as_str) == Some("happy");
            let picture = robot.picture(&BTreeMap::new(), pleased);
            for layer in Layer::ALL {
                let expected: Vec<String> = match case["layers"].get(layer.name()) {
                    Some(rows) => serde_json::from_value(rows.clone()).unwrap(),
                    None => vec![".".repeat(WIDTH); HEIGHT],
                };
                assert_eq!(
                    rows(&picture, layer),
                    expected,
                    "{given:?}, {}",
                    layer.name()
                );
            }
        }
    }

    #[test]
    fn a_robot_is_an_svg_with_named_layers() {
        let robot = Robot::default();
        let svg = robot.picture(&BTreeMap::new(), false).svg(
            &robot.colours,
            &Draw {
                moving: Move::Idle,
                label: "Sam’s <agent>".into(),
                style: false,
            },
        );
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"px a-idle\" viewBox=\"0 0 24 27\" shape-rendering=\"crispEdges\" role=\"img\" aria-label=\"Sam’s &lt;agent&gt;\">"), "{svg}");
        for group in [
            "<g class=\"all\"><g class=\"legs\">",
            "<g class=\"up\"><g class=\"base\">",
            "<g class=\"eyes eo\">",
            "<g class=\"shut ec\">",
            "<g class=\"light\">",
            "<g class=\"arm-left-down al-d\">",
            "<g class=\"arm-left-up al-u\">",
            "<g class=\"arm-right-down ar-d\">",
            "<g class=\"arm-right-up ar-u\">",
            "<path class=\"zz\"",
        ] {
            assert!(svg.contains(group), "{group}");
        }
        // The neck, in the outline's colour, and the chosen colours.
        assert!(svg.contains("M10 15h4v1h-4z"));
        assert!(svg.contains("fill=\"#2b2f3a\""));
        assert!(svg.contains("fill=\"#8ecae6\""));
        assert!(svg.contains("fill=\"#9ff5d6\""));
        // The chest's shine is see-through, as the canvas has it.
        assert!(
            svg.contains("<path d=\"M10 18h1v1h-1z\" fill=\"#ffffff\" opacity=\"0.7\"/>"),
            "{svg}"
        );
        assert!(!svg.contains("<style>"));
        assert!(!svg.contains("flame"));

        let mut hover = Robot::default();
        hover.parts.legs = "hover".into();
        hover.colours.light = "#7c6cf2".into();
        let svg = hover.picture(&BTreeMap::new(), false).svg(
            &hover.colours,
            &Draw {
                moving: Move::Work,
                label: "x".into(),
                style: true,
            },
        );
        assert!(svg.contains("<path class=\"flame\" d=\"M8 23h8v1h-8zM10 24h4v1h-4zM11 25h2v1h-2z\" fill=\"#7c6cf2\" opacity=\"0.75\"/>"), "{svg}");
        assert!(svg.contains("class=\"px a-work\""));
        assert!(svg.contains(&format!("<style>{CSS}</style>")));
    }

    #[test]
    fn a_colour_not_given_is_the_robots_own() {
        let mut colours = Robot::default().colours;
        colours.body = "not a colour".into();
        assert_eq!(colours.of(Role::Body), "#8ecae6");
        colours.body = "#ABCDEF".into();
        assert_eq!(colours.of(Role::Body), "#abcdef");
    }

    #[test]
    fn the_cat_and_the_ghost_are_drawn_from_their_rows() {
        let cat = &library().character("cat").unwrap().character;
        let picture = cat.picture();
        assert_eq!(rows(&picture, Layer::Base), cat.layers[&Layer::Base]);
        assert!(picture.has(Layer::ArmRightUp));
        let ghost = &library().character("ghost").unwrap().character;
        assert!(!ghost.picture().has(Layer::Legs));
        let look = Look {
            character: "ghost".into(),
            ..Look::default()
        };
        let svg = look
            .picture(&BTreeMap::new(), &BTreeMap::new(), Move::Idle)
            .svg(
                &look.robot.colours,
                &Draw {
                    moving: Move::Idle,
                    label: "Ghost".into(),
                    style: false,
                },
            );
        assert!(!svg.contains("class=\"legs\""));
        assert!(svg.contains("<g class=\"light\"><path d=\"M21 1h1v1h-1zM20 2h3v1h-3zM21 3h1v1h-1z\" fill=\"#f4c84a\"/></g>"), "{svg}");
    }

    const HAT: &str = r##"{
  "format": "kelve-part/1",
  "name": "Party hat",
  "slot": "hat",
  "at": [9, 0],
  "colours": { "a": "light", "g": "#f4c84a", "w": "#ffffff" },
  "pixels": [
    "..ww..",
    "..gg..",
    ".aaaa.",
    "agggga"
  ]
}"##;

    #[test]
    fn the_docs_party_hat_is_a_part() {
        let hat = part(HAT).unwrap();
        assert_eq!(hat.name, "Party hat");
        assert_eq!(hat.slot, Slot::Hat);
        assert_eq!(hat.at, [9, 0]);
        assert_eq!(hat.colours[&'a'], Colour::Role(Role::Light));
        assert_eq!(hat.colours[&'g'], Colour::Hex("#f4c84a".into()));
        // On the robot, where the docs put it.
        let mut made = BTreeMap::new();
        made.insert("made-party".to_string(), hat);
        let mut robot = Robot::default();
        robot.parts.hat = "made-party".into();
        let base = rows(&robot.picture(&made, false), Layer::Base);
        let drawn: Vec<&str> = base[..4].iter().map(|r| &r[9..15]).collect();
        assert_eq!(drawn, vec!["..ww..", "..gg..", ".aaaa.", "agggga"]);
        // A hat takes the antenna's place.
        assert!(!robot.picture(&made, false).has(Layer::Light));
        // kelve's own party hat is as the canvas draws it: its gold brim
        // covers the light at its ends.
        robot.parts.hat = "party".into();
        let base = rows(&robot.picture(&made, false), Layer::Base);
        assert_eq!(&base[3][9..15], "gggggg");
    }

    fn part_problems(change: impl Fn(&mut Value)) -> Vec<String> {
        let mut v: Value = serde_json::from_str(HAT).unwrap();
        change(&mut v);
        part(&v.to_string()).unwrap_err()
    }

    #[test]
    fn a_part_is_checked() {
        assert!(part("not json").unwrap_err()[0].contains("isn't JSON"));
        assert!(part("[1]").unwrap_err()[0].contains("one JSON object"));
        let found = part_problems(|v| v["format"] = json!("kelve-part/2"));
        assert_eq!(found, vec!["The part's format is “kelve-part/1”."]);
        assert!(
            part_problems(|v| v["slot"] = json!("tail"))[0].contains("isn't one of the robot's")
        );
        assert!(part_problems(|v| v["at"] = json!([20, 0]))[0].contains("outside its slot's box"));
        assert!(part_problems(|v| v["at"] = json!(["9", 0]))[0].contains("`at` is where"));
        assert!(
            part_problems(|v| v["pixels"][0] = json!("..xx.."))[0]
                .contains("has “x”, which has no colour")
        );
        assert!(part_problems(|v| v["pixels"] = json!(["......"]))[0].contains("has no pixels"));
        assert!(part_problems(|v| v["pixels"] = json!("aaaa"))[0].contains("a list of rows"));
        assert!(
            part_problems(|v| v["colours"]["G"] = json!("#000000"))[0]
                .contains("one lowercase letter")
        );
        assert!(
            part_problems(|v| v["colours"]["g"] = json!("gold"))[0]
                .contains("isn't one kelve knows")
        );
        let found = part_problems(|v| {
            for (i, c) in ["b", "c", "d", "e"].iter().enumerate() {
                v["colours"][*c] = json!(format!("#00000{i}"));
            }
        });
        assert!(
            found[0].contains("7 colours, and the most is 6"),
            "{found:?}"
        );
        assert!(
            part_problems(|v| v["image"] = json!("https://x.com/a.png"))[0]
                .contains("`image`, which kelve doesn't know")
        );
        assert!(
            part_problems(|v| v["name"] = json!("See https://example.com"))[0]
                .contains("has a link")
        );
        assert!(part_problems(|v| v["name"] = json!("<b>Hat</b>"))[0].contains("markup"));
        assert!(part_problems(|v| v["name"] = json!(""))[0].contains("is empty"));
        assert!(part_problems(|v| v["shut"] = json!(["aa"]))[0].contains("Only eyes have `shut`"));
        assert!(part_problems(|v| v["light"] = json!(["aa"]))[0].contains("Only an antenna"));
    }

    #[test]
    fn eyes_blink_and_an_antenna_glows() {
        let eyes = json!({
            "format": "kelve-part/1", "name": "Stars", "slot": "eyes", "at": [8, 7],
            "colours": { "e": "eyes" },
            "pixels": [".e....e.", "eee..eee", ".e....e."],
        });
        assert!(part(&eyes.to_string()).unwrap_err()[0].contains("need `shut`"));
        let mut blinking = eyes.clone();
        blinking["shut"] = json!(["", "", "eee..eee"]);
        let stars = part(&blinking.to_string()).unwrap();
        let mut made = BTreeMap::new();
        made.insert("stars".to_string(), stars);
        let mut robot = Robot::default();
        robot.parts.eyes = "stars".into();
        let picture = robot.picture(&made, false);
        assert_eq!(&rows(&picture, Layer::Eyes)[8][8..16], "eee..eee");
        assert_eq!(&rows(&picture, Layer::Shut)[9][8..16], "eee..eee");
        // While pleased, the robot's own happy eyes.
        let pleased = robot.picture(&made, true);
        assert_eq!(&rows(&pleased, Layer::Eyes)[8][8..16], ".e....e.");

        let antenna = json!({
            "format": "kelve-part/1", "name": "Bulb", "slot": "antenna", "at": [11, 0],
            "colours": { "o": "#2b2f3a", "a": "light" },
            "pixels": ["..", "..", "oo", "oo"], "light": ["aa", "aa"],
        });
        let bulb = part(&antenna.to_string()).unwrap();
        let mut made = BTreeMap::new();
        made.insert("bulb".to_string(), bulb);
        let mut robot = Robot::default();
        robot.parts.antenna = "bulb".into();
        let picture = robot.picture(&made, false);
        assert_eq!(&rows(&picture, Layer::Light)[0][11..13], "aa");
        assert_eq!(&rows(&picture, Layer::Base)[2][11..13], "oo");
        // A hat takes the antenna's place.
        robot.parts.hat = "cap".into();
        assert!(!robot.picture(&made, false).has(Layer::Light));
    }

    #[test]
    fn ears_and_hands_are_drawn_once_and_mirrored() {
        let hand = json!({
            "format": "kelve-part/1", "name": "Mitten", "slot": "hands", "at": [3, 21],
            "colours": { "a": "light" }, "pixels": ["aa.", "aaa"],
        });
        let mitten = part(&hand.to_string()).unwrap();
        let mut made = BTreeMap::new();
        made.insert("mitten".to_string(), mitten);
        let mut robot = Robot::default();
        robot.parts.hands = "mitten".into();
        let picture = robot.picture(&made, false);
        let down = rows(&picture, Layer::ArmLeftDown);
        assert_eq!(&down[21][3..6], "aa.");
        assert_eq!(&down[22][3..6], "aaa");
        let right = rows(&picture, Layer::ArmRightDown);
        assert_eq!(&right[21][18..21], ".aa");
        // Raised, the hand is turned over.
        let up = rows(&picture, Layer::ArmLeftUp);
        assert_eq!(&up[11][2..5], "aaa");
        assert_eq!(&up[12][2..5], "aa.");

        let ear = json!({
            "format": "kelve-part/1", "name": "Fin", "slot": "ears", "at": [1, 8],
            "colours": { "p": "#ff8fab" }, "pixels": ["ppp", ".pp"],
        });
        let fin = part(&ear.to_string()).unwrap();
        let mut made = BTreeMap::new();
        made.insert("fin".to_string(), fin);
        let mut robot = Robot::default();
        robot.parts.ears = "fin".into();
        let base = rows(&robot.picture(&made, false), Layer::Base);
        assert_eq!(&base[8][1..4], "ppp");
        assert_eq!(&base[8][20..23], "ppp");
        assert_eq!(&base[9][1..2], ".");
        assert_eq!(&base[9][22..23], ".");
    }

    #[test]
    fn a_part_for_another_slot_or_none_falls_back_to_the_robots_own() {
        let hat = part(HAT).unwrap();
        let mut made = BTreeMap::new();
        made.insert("party-hat".to_string(), hat);
        let mut robot = Robot::default();
        robot.parts.head = "party-hat".into();
        robot.parts.body = "nothing like it".into();
        let picture = robot.picture(&made, false);
        assert_eq!(picture, Robot::default().picture(&BTreeMap::new(), false));
    }

    const GHOST: &str = r##"{
  "format": "kelve-character/1",
  "name": "Blob",
  "colours": { "o": "#2b2f3a", "b": "body", "e": "eyes" },
  "layers": {
    "base": ["", "", "", "", "", "", "", "", "", "", "........oooooooo", "........obbbbbbo", "........obbbbbbo", "........oooooooo"],
    "eyes": ["", "", "", "", "", "", "", "", "", "", "", "..........e..e"],
    "shut": ["", "", "", "", "", "", "", "", "", "", "", "", "..........e..e"]
  }
}"##;

    #[test]
    fn a_whole_character_is_checked() {
        let blob = character(GHOST).unwrap();
        assert_eq!(blob.name, "Blob");
        assert_eq!(blob.layers.len(), 3);
        let svg = blob.picture().svg(
            &Robot::default().colours,
            &Draw {
                moving: Move::Wave,
                label: "Blob".into(),
                style: false,
            },
        );
        assert!(svg.contains("<g class=\"eyes eo\"><path d=\"M10 11h1v1h-1zM13 11h1v1h-1z\" fill=\"#9ff5d6\"/></g>"), "{svg}");

        let problems = |change: &dyn Fn(&mut Value)| {
            let mut v: Value = serde_json::from_str(GHOST).unwrap();
            change(&mut v);
            character(&v.to_string()).unwrap_err()
        };
        assert!(
            problems(&|v| v["format"] = json!("kelve-part/1"))[0].contains("“kelve-character/1”")
        );
        let found = problems(&|v| {
            v["layers"].as_object_mut().unwrap().remove("shut");
        });
        assert!(found[0].contains("no “shut” layer"), "{found:?}");
        assert!(
            problems(&|v| v["layers"]["tail"] = json!(["b"]))[0].contains("isn't one kelve moves")
        );
        assert!(
            problems(&|v| v["layers"]["base"] = json!(vec!["b"; 28]))[0]
                .contains("28 rows, and the most is 27")
        );
        assert!(
            problems(&|v| v["layers"]["base"] = json!(["b".repeat(25)]))[0]
                .contains("25 pixels wide")
        );
        assert!(problems(&|v| v["layers"]["eyes"] = json!(["...."]))[0].contains("has no pixels"));
        let found = problems(&|v| {
            for c in ["c", "d", "f", "g", "h", "i"] {
                v["colours"][c] = json!("#123456");
            }
        });
        assert!(
            found[0].contains("9 colours, and the most is 8"),
            "{found:?}"
        );
        assert!(problems(&|v| v["script"] = json!("x"))[0].contains("`script`"));
    }

    #[test]
    fn its_name_is_the_persons_until_they_give_it_one() {
        assert_eq!(default_name("Sam"), "Sam’s agent");
        assert_eq!(default_name("  Sam Clark "), "Sam’s agent");
        assert_eq!(default_name(""), "Your agent");
        assert_eq!(name_problem("Bolt"), None);
        assert!(
            name_problem(&"x".repeat(25))
                .unwrap()
                .contains("the most is 24")
        );
        assert!(
            words_problem(
                "The name",
                "ghp_abcdefghijklmnopqrstuvwxyz0123456789AB",
                100
            )
            .unwrap()
            .contains("secret")
        );
        assert!(name_problem("/Users/sam/x").unwrap().contains("path"));
    }

    #[test]
    fn the_robot_looks_by_default_as_the_canvas_has_it() {
        let look = Look::default();
        assert_eq!(look.character, "robot");
        assert_eq!(look.robot.parts.head, "round");
        assert_eq!(look.robot.colours.light, "#ff7a59");
        assert_eq!(look.background, "#e3f2f9");
        assert_eq!(
            serde_json::to_value(Layer::ArmLeftDown).unwrap(),
            "arm-left-down"
        );
        assert_eq!(serde_json::to_value(Move::Happy).unwrap(), "happy");
        assert_eq!(
            serde_json::to_value(Colour::Role(Role::Light)).unwrap(),
            "light"
        );
        assert!(serde_json::from_value::<Colour>(json!("red")).is_err());
    }

    /// A look from a few parts and colours, the rest the robot's own.
    fn look(parts: &[(Slot, &str)], colours: [&str; 4]) -> Look {
        let mut look = Look::default();
        for (slot, key) in parts {
            look.robot.parts.set(*slot, key);
        }
        let [body, light, screen, eyes] = colours.map(str::to_string);
        look.robot.colours = Colours {
            body,
            light,
            screen,
            eyes,
        };
        look
    }

    /// The web app draws characters itself, from `parts.json`, as people
    /// choose how theirs looks: it must draw each exactly as kelve does. This
    /// writes what kelve draws for a case of every kind, and the moves' CSS,
    /// into the web app (`web/src/ui/gen`), whose own test draws each case
    /// again and compares, and whose pages use that CSS.
    #[test]
    fn the_web_app_has_what_kelve_draws_to_compare() {
        let mut made = BTreeMap::new();
        made.insert("made-party".to_string(), part(HAT).unwrap());
        for (key, drawn) in [
            (
                "made-stars",
                json!({
                    "format": "kelve-part/1", "name": "Stars", "slot": "eyes", "at": [8, 7],
                    "colours": { "e": "eyes", "w": "#FFFFFF" },
                    "pixels": [".e....e.", "eee..eee", ".ew...e."], "shut": ["", "", "eee..eee"],
                }),
            ),
            (
                "made-bulb",
                json!({
                    "format": "kelve-part/1", "name": "Bulb", "slot": "antenna", "at": [11, 0],
                    "colours": { "o": "#2b2f3a", "a": "light" },
                    "pixels": ["..", "..", "oo", "oo"], "light": ["aa", "aa"],
                }),
            ),
            (
                "made-mitten",
                json!({
                    "format": "kelve-part/1", "name": "Mitten", "slot": "hands", "at": [3, 21],
                    "colours": { "a": "light", "o": "#2b2f3a" }, "pixels": ["aa.", "aoa"],
                }),
            ),
            (
                "made-fin",
                json!({
                    "format": "kelve-part/1", "name": "Fin", "slot": "ears", "at": [1, 8],
                    "colours": { "p": "#ff8fab" }, "pixels": ["ppp", ".pp"],
                }),
            ),
        ] {
            made.insert(key.to_string(), part(&drawn.to_string()).unwrap());
        }
        let mut characters = BTreeMap::new();
        characters.insert("made-blob".to_string(), character(GHOST).unwrap());
        let none = BTreeMap::new();

        let mut cases: Vec<(String, Look, bool, Move, String, bool)> = Vec::new();
        let mut case = |name: String, look: Look, with_made: bool, moving: Move| {
            cases.push((name, look, with_made, moving, "Your agent".into(), false));
        };
        for moving in [
            Move::Idle,
            Move::Work,
            Move::Happy,
            Move::Wave,
            Move::Sleep,
            Move::Still,
        ] {
            case(
                format!("the robot, {}", moving.name()),
                Look::default(),
                false,
                moving,
            );
        }
        for slot in Slot::ALL {
            for option in &library().slot(slot).unwrap().options {
                let mut look = Look::default();
                look.robot.parts.set(slot, &option.key);
                // A hat hides the antenna; with none, the antenna shows.
                case(
                    format!("{} {}", slot.name(), option.key),
                    look,
                    false,
                    Move::Still,
                );
            }
        }
        case(
            "Sunny".into(),
            look(
                &[
                    (Slot::Head, "box"),
                    (Slot::Hat, "beanie"),
                    (Slot::Eyes, "dots"),
                    (Slot::Mouth, "flat"),
                    (Slot::Ears, "cups"),
                    (Slot::Body, "round"),
                    (Slot::Chest, "heart"),
                    (Slot::Hands, "claws"),
                    (Slot::Legs, "wheels"),
                ],
                ["#ffd166", "#7c6cf2", "#1d1c1a", "#ffffff"],
            ),
            false,
            Move::Happy,
        );
        case(
            "Mint".into(),
            look(
                &[
                    (Slot::Head, "bucket"),
                    (Slot::Antenna, "double"),
                    (Slot::Eyes, "visor"),
                    (Slot::Mouth, "none"),
                    (Slot::Ears, "none"),
                    (Slot::Body, "slim"),
                    (Slot::Legs, "hover"),
                ],
                ["#b8e0a8", "#2ec4b6", "#123b3a", "#ffd166"],
            ),
            false,
            Move::Work,
        );
        let mut odd = Look::default();
        odd.robot.colours.body = "not a colour".into();
        odd.robot.colours.eyes = "#ABCDEF".into();
        case("colours not given".into(), odd, false, Move::Idle);
        for key in ["cat", "ghost"] {
            let own = library().character(key).unwrap();
            for moving in [Move::Idle, Move::Happy, Move::Sleep] {
                let mut look = Look {
                    character: key.into(),
                    ..Look::default()
                };
                look.robot.colours = own.colours.clone();
                case(format!("the {key}, {}", moving.name()), look, false, moving);
            }
        }
        let wearing = look(
            &[
                (Slot::Hat, "none"),
                (Slot::Antenna, "made-bulb"),
                (Slot::Eyes, "made-stars"),
                (Slot::Hands, "made-mitten"),
                (Slot::Ears, "made-fin"),
            ],
            ["#f4a6c0", "#2ec4b6", "#3b2a5c", "#ffffff"],
        );
        for moving in [Move::Idle, Move::Happy] {
            case(
                format!("parts its agent drew, {}", moving.name()),
                wearing.clone(),
                true,
                moving,
            );
        }
        case(
            "a party hat its agent drew".into(),
            look(
                &[(Slot::Hat, "made-party")],
                ["#8ecae6", "#7c6cf2", "#22304a", "#9ff5d6"],
            ),
            true,
            Move::Wave,
        );
        case(
            "a character its agent drew".into(),
            Look {
                character: "made-blob".into(),
                ..Look::default()
            },
            true,
            Move::Wave,
        );
        case(
            "a character that isn't there".into(),
            Look {
                character: "made-gone".into(),
                ..Look::default()
            },
            true,
            Move::Idle,
        );
        cases.push((
            "named, with its own style".into(),
            Look::default(),
            false,
            Move::Sleep,
            "Sam’s <agent> & \"co\"".into(),
            true,
        ));

        let cases: Vec<Value> = cases
            .into_iter()
            .map(|(name, look, with_made, moving, label, style)| {
                let (parts, drawn) = if with_made {
                    (&made, &characters)
                } else {
                    (&none, &BTreeMap::new())
                };
                let svg = look.svg(parts, drawn, moving, &label, style);
                json!({
                    "name": name,
                    "look": look,
                    "parts": parts,
                    "characters": drawn,
                    "move": moving,
                    "label": label,
                    "style": style,
                    "svg": svg,
                })
            })
            .collect();
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/ui/gen");
        std::fs::create_dir_all(&out).unwrap();
        let write = |file: &str, text: String| {
            let path = out.join(file);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                std::fs::write(&path, text).unwrap();
            }
        };
        write(
            "character.json",
            serde_json::to_string_pretty(&json!({ "cases": cases })).unwrap() + "\n",
        );
        write(
            "character.css",
            format!(
                "/* How every character moves, written by kelve's own tests from\n   kelve_core::character::CSS. Don't edit it here. */\n{CSS}\n"
            ),
        );
    }

    #[test]
    fn a_look_is_checked_and_a_drawing_read_by_its_format() {
        let mut made = BTreeMap::new();
        made.insert("made-party".to_string(), part(HAT).unwrap());
        let none = BTreeMap::new();
        let no = BTreeMap::new();
        assert_eq!(Look::default().problem(&none, &no), None);
        let mut look = Look::default();
        look.robot.parts.hat = "made-party".into();
        assert_eq!(look.problem(&made, &no), None);
        assert!(look.problem(&none, &no).unwrap().contains("robot's hat"));
        look.robot.parts.head = "made-party".into();
        assert!(look.problem(&made, &no).unwrap().contains("robot's head"));
        let cat = Look {
            character: "cat".into(),
            ..Look::default()
        };
        assert_eq!(cat.problem(&none, &no), None);
        let gone = Look {
            character: "made-gone".into(),
            ..Look::default()
        };
        assert!(gone.problem(&none, &no).unwrap().contains("character"));
        let grey = Look {
            background: "grey".into(),
            ..Look::default()
        };
        assert!(grey.problem(&none, &no).unwrap().contains("#rrggbb"));

        assert!(matches!(drawing(HAT), Ok(Drawing::Part(_))));
        assert!(matches!(drawing(GHOST), Ok(Drawing::Character(_))));
        assert!(drawing("{\"format\": \"kelve-hat/1\"}").unwrap_err()[0].contains("kelve-part/1"));
        assert!(drawing("not json").unwrap_err()[0].contains("isn't JSON"));
    }
}
