//! Device-independent identities, commands, and logical geometry for FluxWM.
mod action;
mod arena;
pub use action::*;
pub use arena::*;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("expected a generational identity in index:generation form")]
pub struct InvalidId;

macro_rules! identity {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(into = "String", try_from = "String")]
        pub struct $name {
            index: u32,
            generation: u32,
        }
        impl $name {
            pub const fn new(index: u32, generation: u32) -> Self {
                Self { index, generation }
            }
            pub const fn index(self) -> u32 {
                self.index
            }
            pub const fn generation(self) -> u32 {
                self.generation
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", self.index, self.generation)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
        impl FromStr for $name {
            type Err = InvalidId;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (index, generation) = s.split_once(':').ok_or(InvalidId)?;
                if index.is_empty()
                    || generation.is_empty()
                    || !index.bytes().all(|c| c.is_ascii_digit())
                    || !generation.bytes().all(|c| c.is_ascii_digit())
                {
                    return Err(InvalidId);
                }
                Ok(Self::new(
                    index.parse().map_err(|_| InvalidId)?,
                    generation.parse().map_err(|_| InvalidId)?,
                ))
            }
        }
        impl TryFrom<String> for $name {
            type Error = InvalidId;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }
    };
}
identity!(WindowId);
identity!(WorkspaceId);
identity!(OutputId);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite)
            && self.width >= 0.0
            && self.height >= 0.0
    }
    pub fn contains(self, p: Point) -> bool {
        self.is_valid()
            && p.x >= self.x
            && p.y >= self.y
            && p.x < self.x + self.width
            && p.y < self.y + self.height
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    Strip,
    Canvas,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_cannot_alias_generations_and_round_trip_without_json_precision_loss() {
        let a = WindowId::new(u32::MAX, u32::MAX);
        let b = WindowId::new(u32::MAX, 0);
        assert_ne!(a, b);
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, "\"4294967295:4294967295\"");
        assert_eq!(serde_json::from_str::<WindowId>(&json).unwrap(), a);
        assert!("-1:0".parse::<WindowId>().is_err());
        assert!("1:2:3".parse::<WindowId>().is_err());
    }
    #[test]
    fn geometry_rejects_nonfinite_and_uses_half_open_edges() {
        let r = Rect {
            x: 1.,
            y: 2.,
            width: 3.,
            height: 4.,
        };
        assert!(r.contains(Point { x: 1., y: 2. }));
        assert!(!r.contains(Point { x: 4., y: 2. }));
        assert!(
            !Rect {
                width: f64::INFINITY,
                ..r
            }
            .is_valid()
        );
    }
}
