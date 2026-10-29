//! Fixed-function state defaults and validation from the original runtime.
use std::collections::BTreeMap;
#[derive(Clone)]
pub(super) struct StateBank {
    kind: u32,
    values: BTreeMap<u32, u32>,
}
impl StateBank {
    pub fn new(kind: u32) -> Self {
        let defaults: &[(u32, u32)] = match kind {
            0 => &[
                (2, 0),
                (4, 1),
                (8, 3),
                (9, 2),
                (26, 0),
                (28, 0),
                (29, 0),
                (34, 0),
                (139, 0),
                (7, 0),
                (14, 1),
                (15, 0),
                (19, 2),
                (20, 1),
                (22, 3),
                (23, 4),
                (24, 0),
                (25, 8),
                (27, 0),
            ],
            1 => &[
                (1, 4),
                (2, 2),
                (3, 1),
                (4, 2),
                (5, 2),
                (6, 1),
                (11, 0),
                (12, 1),
                (13, 1),
                (14, 1),
                (15, 0),
                (16, 1),
                (17, 1),
                (18, 1),
                (19, 0),
                (20, 0),
                (21, 1),
                (24, 0),
            ],
            _ => &[
                (1, 0),
                (2, 0),
                (3, 2),
                (4, 0),
                (5, 0),
                (6, 0x3f800000),
                (7, 0x3f800000),
                (8, 0),
            ],
        };
        Self {
            kind,
            values: defaults.iter().copied().collect(),
        }
    }
    pub fn get(&self, key: u32) -> Option<u32> {
        self.values.get(&key).copied()
    }
    pub fn set(&mut self, key: u32, value: u32, known_resource: bool) -> u32 {
        if !self.values.contains_key(&key) {
            return 0x80004001;
        }
        let valid = match self.kind {
            0 => match key {
                34 => true,
                2 | 26 | 28 | 29 => value == 0,
                4 => value == 1,
                8 => value == 3,
                9 => value == 2,
                22 => (1..=3).contains(&value),
                23 | 25 => (1..=8).contains(&value),
                19 | 20 => (1..=11).contains(&value),
                24 => value <= 255,
                _ => value <= 1,
            },
            1 => match key {
                1 | 4 => matches!(value, 1 | 2 | 3 | 4 | 7 | 13),
                2 | 3 | 5 | 6 => value & !0x33 == 0,
                12..=14 => (1..=4).contains(&value),
                16 | 17 => value == 1 || value == 2,
                15 => true,
                18 | 21 => value == 1,
                11 | 19 | 20 | 24 => value == 0,
                _ => false,
            },
            _ => {
                !((key == 1 && value != 0 && !known_resource)
                    || (key == 3 && value != 2)
                    || (key == 4 && value != 0)
                    || (key == 8 && value > 1))
            }
        };
        if !valid {
            return if self.kind == 0 {
                0x80070057
            } else {
                0x80004001
            };
        }
        self.values.insert(key, value);
        if self.kind == 1 && key == 12 {
            self.values.insert(13, value);
            self.values.insert(14, value);
        }
        0
    }
}
pub(super) fn identity() -> [f64; 16] {
    std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 })
}
