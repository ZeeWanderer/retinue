use super::Command;
use std::rc::Rc;

impl PartialEq for Command {
    fn eq(&self, other: &Self) -> bool {
        use Command::*;
        match (self, other) {
            (
                Fragment {
                    program: a,
                    context: ac,
                },
                Fragment {
                    program: b,
                    context: bc,
                },
            ) => a.source_eq(b) && ac == bc,
            (
                Ring {
                    center: a,
                    radius: b,
                    thickness: c,
                    color: d,
                },
                Ring {
                    center: w,
                    radius: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                Translate {
                    offset: a,
                    commands: b,
                },
                Translate {
                    offset: x,
                    commands: y,
                },
            ) => a == x && (Rc::ptr_eq(b, y) || b == y),
            (
                Fill {
                    bounds: a,
                    radius: b,
                    color: c,
                },
                Fill {
                    bounds: x,
                    radius: y,
                    color: z,
                },
            ) => (a, b, c) == (x, y, z),
            (
                Outline {
                    bounds: a,
                    radius: b,
                    thickness: c,
                    color: d,
                },
                Outline {
                    bounds: w,
                    radius: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                InsetShadow {
                    bounds: a,
                    radius: b,
                    blur: c,
                    spread: d,
                    color: e,
                },
                InsetShadow {
                    bounds: v,
                    radius: w,
                    blur: x,
                    spread: y,
                    color: z,
                },
            ) => (a, b, c, d, e) == (v, w, x, y, z),
            (
                Text {
                    bounds: a,
                    size: b,
                    text: c,
                    color: d,
                    align: e,
                    font: f,
                },
                Text {
                    bounds: u,
                    size: v,
                    text: w,
                    color: x,
                    align: y,
                    font: z,
                },
            ) => (a, b, c, d, e) == (u, v, w, x, y) && f.id() == z.id(),
            (
                Image {
                    bounds: a,
                    image: b,
                    fit: c,
                },
                Image {
                    bounds: x,
                    image: y,
                    fit: z,
                },
            ) => (a, b, c) == (x, y, z),
            (
                Clip {
                    bounds: a,
                    commands: b,
                },
                Clip {
                    bounds: x,
                    commands: y,
                },
            ) => a == x && (Rc::ptr_eq(b, y) || b == y),
            (
                Line {
                    from: a,
                    to: b,
                    thickness: c,
                    color: d,
                },
                Line {
                    from: w,
                    to: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                Polygon {
                    points: a,
                    color: b,
                },
                Polygon {
                    points: x,
                    color: y,
                },
            ) => (a, b) == (x, y),
            (External { bounds: a, key: b }, External { bounds: x, key: y }) => (a, b) == (x, y),
            _ => false,
        }
    }
}

impl Command {
    pub(super) fn same_paint(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        let same = |a: &[Command], b: &[Command]| {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same_paint(b))
        };
        match (self, other) {
            (Self::Fragment { program: a, .. }, Self::Fragment { program: b, .. }) => {
                a.scale() == b.scale() && same(a.commands(), b.commands())
            }
            (
                Self::Translate {
                    offset: a,
                    commands: ac,
                },
                Self::Translate {
                    offset: b,
                    commands: bc,
                },
            ) => a == b && same(ac, bc),
            (
                Self::Clip {
                    bounds: a,
                    commands: ac,
                },
                Self::Clip {
                    bounds: b,
                    commands: bc,
                },
            ) => a == b && same(ac, bc),
            _ => false,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Text { text, .. } => text.capacity(),
                Self::Polygon { points, .. } => points.capacity() * size_of::<[f32; 2]>(),
                Self::Clip { commands, .. } | Self::Translate { commands, .. } => {
                    commands.iter().map(Self::retained_bytes).sum()
                }
                _ => 0,
            }
    }
}
