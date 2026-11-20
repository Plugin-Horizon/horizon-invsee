//! The live view's clicks: vanilla's container click rules (`AbstractContainerMenu.doClick`) worked out on a
//! board of the target's shown slots, the spectator's inventory and the cursor, before anything is written.
//! The game moves nothing in a mod's menu, so the mod does what the click would have done.
use super::{inventory_slot_below, Host};

/// A drag (quick craft, click mode 5) that has started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drag {
    /// 0 spread evenly, 1 one each, 2 creative clone (not supported here).
    pub kind: u8,
    /// The menu slots the drag passed over.
    pub slots: Vec<i32>,
}

/// What a click works on.
#[derive(Clone, Debug, PartialEq)]
pub struct Board<S> {
    /// The menu's own slots: the target's stack where the slot shows one.
    pub own: Vec<Option<S>>,
    /// Own slots that take nothing and give nothing (placeholders, the cursor, the crafting grid).
    pub locked: Vec<bool>,
    /// The spectator's 36 inventory slots (0..9 the hotbar).
    pub inv: Vec<Option<S>>,
    /// The spectator's cursor.
    pub cursor: Option<S>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    Own(usize),
    Inv(usize),
}

impl<S: Clone> Board<S> {
    fn loc(&self, slot: i32) -> Option<Loc> {
        let own = self.own.len();
        match usize::try_from(slot) {
            Ok(s) if s < own => Some(Loc::Own(s)),
            _ => inventory_slot_below(own, slot).map(Loc::Inv),
        }
    }

    fn get(&self, l: Loc) -> Option<&S> {
        match l {
            Loc::Own(i) => self.own[i].as_ref(),
            Loc::Inv(i) => self.inv[i].as_ref(),
        }
    }

    fn set(&mut self, l: Loc, v: Option<S>) {
        match l {
            Loc::Own(i) => self.own[i] = v,
            Loc::Inv(i) => self.inv[i] = v,
        }
    }

    fn locked(&self, l: Loc) -> bool {
        match l {
            Loc::Own(i) => self.locked[i],
            Loc::Inv(_) => false,
        }
    }

    /// Every slot in the menu's order: the own ones, then the inventory's three rows, then the hotbar.
    fn order(&self) -> Vec<Loc> {
        let mut out: Vec<Loc> = (0..self.own.len()).map(Loc::Own).collect();
        out.extend((9..36).map(Loc::Inv));
        out.extend((0..9).map(Loc::Inv));
        out
    }
}

fn count<H: Host>(h: &H, s: &H::Stack) -> u32 {
    h.describe(s).1
}

fn max<H: Host>(h: &H, s: &H::Stack) -> u32 {
    h.max_stack(s).max(1)
}

/// `s` with `n` items; nothing for 0.
fn with<H: Host>(h: &H, s: &H::Stack, n: u32) -> Option<H::Stack> {
    if n == 0 { None } else { h.with_count(s, n) }
}

/// A click on menu slot `slot` (`-999` outside) with `button` and `mode` (vanilla's `ClickType` id): whether
/// the board changed.
pub fn click<H: Host>(
    h: &H,
    b: &mut Board<H::Stack>,
    drag: &mut Option<Drag>,
    slot: i32,
    button: i32,
    mode: i32,
) -> bool {
    match mode {
        0 => pickup(h, b, slot, button),
        1 => quick_move(h, b, slot),
        2 => swap(h, b, slot, button),
        5 => quick_craft(h, b, drag, slot, button),
        6 => pickup_all(h, b, slot, button),
        // Middle click (creative), throw and the rest: nothing the board can carry out.
        _ => false,
    }
}

/// Left or right click: pick up, put down, merge or swap with the cursor.
fn pickup<H: Host>(h: &H, b: &mut Board<H::Stack>, slot: i32, button: i32) -> bool {
    let Some(l) = b.loc(slot) else { return false };
    if b.locked(l) {
        return false;
    }
    let here = b.get(l).cloned();
    let held = b.cursor.clone();
    let right = button == 1;
    match (held, here) {
        (None, None) => false,
        (None, Some(s)) => {
            let n = count(h, &s);
            let take = if right { n.div_ceil(2) } else { n };
            let (Some(took), rest) = (with(h, &s, take), with(h, &s, n - take)) else { return false };
            b.cursor = Some(took);
            b.set(l, rest);
            true
        }
        (Some(c), None) => {
            let n = count(h, &c);
            let put = if right { 1 } else { n }.min(max(h, &c));
            let (Some(placed), rest) = (with(h, &c, put), with(h, &c, n - put)) else { return false };
            b.set(l, Some(placed));
            b.cursor = rest;
            true
        }
        (Some(c), Some(s)) if h.same_stack(&c, &s) => {
            let (n, here_n) = (count(h, &c), count(h, &s));
            let room = max(h, &s).saturating_sub(here_n);
            let put = if right { 1 } else { n }.min(room);
            if put == 0 {
                return false;
            }
            let (Some(merged), rest) = (with(h, &s, here_n + put), with(h, &c, n - put)) else { return false };
            b.set(l, Some(merged));
            b.cursor = rest;
            true
        }
        (Some(c), Some(s)) => {
            // A different stack: they swap places when the cursor's fits the slot whole.
            if count(h, &c) > max(h, &c) {
                return false;
            }
            b.set(l, Some(c));
            b.cursor = Some(s);
            true
        }
    }
}

/// `moveItemStackTo`: the stack at `from` into `to` (in that order), onto matching stacks first, then into
/// empty slots. Whether anything moved.
fn move_to<H: Host>(h: &H, b: &mut Board<H::Stack>, from: Loc, to: &[Loc]) -> bool {
    let Some(src) = b.get(from).cloned() else { return false };
    let mut left = count(h, &src);
    let start = left;
    for &t in to {
        if left == 0 {
            break;
        }
        if t == from || b.locked(t) {
            continue;
        }
        let Some(there) = b.get(t).cloned() else { continue };
        if !h.same_stack(&src, &there) {
            continue;
        }
        let n = count(h, &there);
        let room = max(h, &there).saturating_sub(n);
        let put = room.min(left);
        if put > 0 {
            if let Some(merged) = with(h, &there, n + put) {
                b.set(t, Some(merged));
                left -= put;
            }
        }
    }
    for &t in to {
        if left == 0 {
            break;
        }
        if t == from || b.locked(t) || b.get(t).is_some() {
            continue;
        }
        let put = max(h, &src).min(left);
        if let Some(placed) = with(h, &src, put) {
            b.set(t, Some(placed));
            left -= put;
        }
    }
    if left == start {
        return false;
    }
    b.set(from, with(h, &src, left));
    true
}

/// Shift click: the stack goes to the other side of the menu (own slots to the spectator's inventory,
/// last slots first, and the other way, first slots first), as `quickMoveStack` does.
fn quick_move<H: Host>(h: &H, b: &mut Board<H::Stack>, slot: i32) -> bool {
    let Some(from) = b.loc(slot) else { return false };
    if b.locked(from) || b.get(from).is_none() {
        return false;
    }
    let order = b.order();
    let own = b.own.len();
    let to: Vec<Loc> = match from {
        Loc::Own(_) => order[own..].iter().rev().copied().collect(),
        Loc::Inv(_) => order[..own].to_vec(),
    };
    move_to(h, b, from, &to)
}

/// A number key: the slot's stack and the hotbar slot's change places.
fn swap<H: Host>(h: &H, b: &mut Board<H::Stack>, slot: i32, button: i32) -> bool {
    let Some(l) = b.loc(slot) else { return false };
    let Ok(hot) = usize::try_from(button) else { return false };
    if hot >= 9 || b.locked(l) || l == Loc::Inv(hot) {
        return false;
    }
    let (a, c) = (b.get(l).cloned(), b.inv[hot].clone());
    if a.is_none() && c.is_none() {
        return false;
    }
    // A stack past the item's maximum would not fit a slot of the other side whole.
    if a.as_ref().is_some_and(|s| count(h, s) > max(h, s)) || c.as_ref().is_some_and(|s| count(h, s) > max(h, s)) {
        return false;
    }
    b.set(l, c);
    b.inv[hot] = a;
    true
}

/// Double click: every matching stack is gathered onto the cursor, partial stacks first.
fn pickup_all<H: Host>(h: &H, b: &mut Board<H::Stack>, slot: i32, button: i32) -> bool {
    if button != 0 || b.loc(slot).is_none() {
        return false;
    }
    let Some(c) = b.cursor.clone() else { return false };
    let limit = max(h, &c);
    let mut have = count(h, &c);
    let before = have;
    let order = b.order();
    for full in [false, true] {
        for &l in &order {
            if have >= limit {
                break;
            }
            if b.locked(l) {
                continue;
            }
            let Some(s) = b.get(l).cloned() else { continue };
            let n = count(h, &s);
            if !h.same_stack(&c, &s) || (!full && n >= max(h, &s)) {
                continue;
            }
            let take = n.min(limit - have);
            b.set(l, with(h, &s, n - take));
            have += take;
        }
    }
    if have == before {
        return false;
    }
    b.cursor = with(h, &c, have);
    true
}

/// Whether a stack can be spread onto `l` (`canItemQuickReplace` with the size ignored, `mayPlace`).
fn can_spread<H: Host>(h: &H, b: &Board<H::Stack>, l: Loc, c: &H::Stack) -> bool {
    if b.locked(l) {
        return false;
    }
    match b.get(l) {
        None => true,
        Some(s) => h.same_stack(c, s),
    }
}

/// A drag: start (button 0, 4, 8), a slot added (1, 5, 9) and end (2, 6, 10). The end spreads the cursor.
fn quick_craft<H: Host>(
    h: &H,
    b: &mut Board<H::Stack>,
    drag: &mut Option<Drag>,
    slot: i32,
    button: i32,
) -> bool {
    let (stage, kind) = (button & 3, ((button >> 2) & 3) as u8);
    match stage {
        0 => {
            *drag = (kind < 2).then(|| Drag { kind, slots: Vec::new() });
            false
        }
        1 => {
            if let (Some(d), Some(c), Some(l)) = (drag.as_mut(), b.cursor.as_ref(), b.loc(slot)) {
                if d.kind == kind && can_spread(h, b, l, c) && !d.slots.contains(&slot) {
                    d.slots.push(slot);
                }
            }
            false
        }
        2 => {
            let Some(d) = drag.take() else { return false };
            let Some(c) = b.cursor.clone() else { return false };
            if d.slots.len() == 1 {
                // One slot is a plain click: all of the cursor's stack, or one.
                return pickup(h, b, d.slots[0], i32::from(d.kind));
            }
            let total = count(h, &c);
            let n = d.slots.len() as u32;
            if n == 0 || total < n {
                return false;
            }
            let each = if d.kind == 0 { total / n } else { 1 };
            let mut left = total;
            let mut changed = false;
            for slot in d.slots {
                let Some(l) = b.loc(slot) else { continue };
                if !can_spread(h, b, l, &c) {
                    continue;
                }
                let have = b.get(l).map_or(0, |s| count(h, s));
                let now = (have + each).min(max(h, &c));
                let put = now - have;
                if put == 0 {
                    continue;
                }
                if let Some(s) = with(h, &c, now) {
                    b.set(l, Some(s));
                    left -= put;
                    changed = true;
                }
            }
            b.cursor = with(h, &c, left);
            changed
        }
        _ => false,
    }
}
