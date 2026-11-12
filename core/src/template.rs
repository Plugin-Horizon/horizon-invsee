//! The layout of a view: which of the target's slots each of the menu's slots shows, from the
//! `[templates]` text (one line per row of nine, words separated by spaces).
use crate::Kind;

pub const DEFAULT_INVENTORY: &str = "\
i_00 i_01 i_02 i_03 i_04 i_05 i_06 i_07 i_08
i_09 i_10 i_11 i_12 i_13 i_14 i_15 i_16 i_17
i_18 i_19 i_20 i_21 i_22 i_23 i_24 i_25 i_26
i_27 i_28 i_29 i_30 i_31 i_32 i_33 i_34 i_35
a_b  a_l  a_c  a_h  oh   b    s    c    _
p_00 p_01 p_02 p_03 p_04 p_05 p_06 p_07 p_08
";

pub const DEFAULT_ENDERCHEST: &str = "\
e_00 e_01 e_02 e_03 e_04 e_05 e_06 e_07 e_08
e_09 e_10 e_11 e_12 e_13 e_14 e_15 e_16 e_17
e_18 e_19 e_20 e_21 e_22 e_23 e_24 e_25 e_26
e_27 e_28 e_29 e_30 e_31 e_32 e_33 e_34 e_35
e_36 e_37 e_38 e_39 e_40 e_41 e_42 e_43 e_44
e_45 e_46 e_47 e_48 e_49 e_50 e_51 e_52 e_53
";

/// Horizon's slot numbers of a player inventory (`NativeInventoryKind::Player`).
pub const FEET: usize = 36;
pub const LEGS: usize = 37;
pub const CHEST: usize = 38;
pub const HEAD: usize = 39;
pub const OFF_HAND: usize = 40;

/// Slots of the target that Horizon cannot reach (yet): shown, never filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unavailable {
    Body,
    Saddle,
    Cursor,
    Crafting(u8),
}

/// One slot of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// The target's slot (Horizon's numbering for the view's kind).
    Slot(usize),
    /// A slot the original shows that this engine can't reach.
    Unavailable(Unavailable),
    /// Nothing.
    Unused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Rows of nine.
    pub cells: Vec<Cell>,
}

impl Layout {
    pub fn rows(&self) -> u8 {
        (self.cells.len() / 9) as u8
    }

    /// The menu slot showing the target's `slot`, if any.
    pub fn menu_slot_of(&self, slot: usize) -> Option<usize> {
        self.cells.iter().position(|c| *c == Cell::Slot(slot))
    }

    /// `text` read for `kind`, with the warnings for what it skipped. Unknown words and slots
    /// the kind lacks become unused slots; rows past six are left out, short rows padded; the
    /// trailing rows that show nothing are dropped (an ender chest has three of the default's
    /// six). At least one row is kept.
    pub fn parse(text: &str, kind: Kind) -> (Layout, Vec<String>) {
        let mut warnings = Vec::new();
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            if rows.len() == 6 {
                warnings.push("templates: more than six rows, the rest is left out".into());
                break;
            }
            let mut row: Vec<Cell> = line
                .split_whitespace()
                .map(|w| {
                    word(w, kind).unwrap_or_else(|e| {
                        warnings.push(format!("templates: {e}"));
                        Cell::Unused
                    })
                })
                .collect();
            if row.len() > 9 {
                warnings.push(format!("templates: `{}` has more than nine slots", line.trim()));
            }
            row.resize(9, Cell::Unused);
            rows.push(row);
        }
        while rows.len() > 1
            && rows
                .last()
                .is_some_and(|r| r.iter().all(|c| *c == Cell::Unused))
        {
            rows.pop();
        }
        if rows.is_empty() {
            rows.push(vec![Cell::Unused; 9]);
        }
        let mut seen = std::collections::HashSet::new();
        for c in rows.iter_mut().flatten() {
            if let Cell::Slot(i) = *c {
                if !seen.insert(i) {
                    warnings.push(format!("templates: slot {i} shown twice, the second is unused"));
                    *c = Cell::Unused;
                }
            }
        }
        (
            Layout {
                cells: rows.into_iter().flatten().collect(),
            },
            warnings,
        )
    }
}

fn number(s: &str, max: usize, w: &str) -> Result<usize, String> {
    let n: usize = s.parse().map_err(|_| format!("`{w}` is not a slot"))?;
    if n < max {
        Ok(n)
    } else {
        Err(format!("`{w}`: there is no such slot here"))
    }
}

fn word(w: &str, kind: Kind) -> Result<Cell, String> {
    use Unavailable::*;
    match kind {
        Kind::Inventory => Ok(match w {
            "_" => Cell::Unused,
            "a_b" => Cell::Slot(FEET),
            "a_l" => Cell::Slot(LEGS),
            "a_c" => Cell::Slot(CHEST),
            "a_h" => Cell::Slot(HEAD),
            "oh" => Cell::Slot(OFF_HAND),
            "b" => Cell::Unavailable(Body),
            "s" => Cell::Unavailable(Saddle),
            "c" => Cell::Unavailable(Cursor),
            _ => {
                if let Some(n) = w.strip_prefix("i_") {
                    Cell::Slot(number(n, 36, w)?)
                } else if let Some(n) = w.strip_prefix("p_") {
                    // The 2x2 grid and the result, or a station's slots: up to nine.
                    Cell::Unavailable(Crafting(number(n, 9, w)? as u8))
                } else {
                    return Err(format!("`{w}` is not an inventory slot"));
                }
            }
        }),
        Kind::Ender => match w {
            "_" => Ok(Cell::Unused),
            _ => match w.strip_prefix("e_") {
                // Vanilla's ender chest has 27 slots; the original's 54 exist on Purpur only.
                Some(n) => match n.parse::<usize>() {
                    Ok(n) if n < 27 => Ok(Cell::Slot(n)),
                    Ok(_) => Ok(Cell::Unused),
                    Err(_) => Err(format!("`{w}` is not a slot")),
                },
                None => Err(format!("`{w}` is not an ender chest slot")),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_inventory_shows_every_reachable_slot_once() {
        let (l, w) = Layout::parse(DEFAULT_INVENTORY, Kind::Inventory);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(l.rows(), 6);
        for i in 0..41 {
            assert!(l.menu_slot_of(i).is_some(), "slot {i}");
        }
        assert_eq!(l.menu_slot_of(0), Some(0));
        assert_eq!(l.menu_slot_of(35), Some(35));
        assert_eq!(l.cells[36], Cell::Slot(FEET));
        assert_eq!(l.cells[39], Cell::Slot(HEAD));
        assert_eq!(l.cells[40], Cell::Slot(OFF_HAND));
        assert_eq!(l.cells[43], Cell::Unavailable(Unavailable::Cursor));
        assert_eq!(l.cells[44], Cell::Unused);
        assert_eq!(l.cells[45], Cell::Unavailable(Unavailable::Crafting(0)));
    }

    #[test]
    fn the_default_ender_chest_is_three_rows() {
        let (l, w) = Layout::parse(DEFAULT_ENDERCHEST, Kind::Ender);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(l.rows(), 3);
        assert_eq!(l.cells.len(), 27);
        assert_eq!(l.cells[26], Cell::Slot(26));
    }

    #[test]
    fn bad_words_become_unused_with_a_warning() {
        let (l, w) = Layout::parse("i_00 zz i_99 e_01 i_00\n", Kind::Inventory);
        assert_eq!(l.rows(), 1);
        assert_eq!(l.cells[0], Cell::Slot(0));
        assert_eq!(&l.cells[1..5], &[Cell::Unused; 4]);
        assert_eq!(w.len(), 4, "{w:?}");
        let (l, _) = Layout::parse("", Kind::Ender);
        assert_eq!(l.rows(), 1);
    }
}
