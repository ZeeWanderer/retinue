use super::*;
use std::hash::{Hash, Hasher};

pub(super) fn fingerprint(commands: &[Command], size: (f32, f32), scale: f32) -> u64 {
    let mut hash = std::hash::DefaultHasher::new();
    (size.0.to_bits(), size.1.to_bits(), scale.to_bits()).hash(&mut hash);
    block(commands, &mut hash);
    hash.finish()
}

fn floats(values: &[f32], hash: &mut impl Hasher) {
    for value in values {
        (if *value == 0.0 { 0 } else { value.to_bits() }).hash(hash);
    }
}

fn bounds(b: &super::super::Bounds, hash: &mut impl Hasher) {
    floats(&[b.x, b.y, b.width, b.height], hash);
}

fn block(commands: &[Command], hash: &mut impl Hasher) {
    commands.len().hash(hash);
    for command in commands {
        std::mem::discriminant(command).hash(hash);
        match command {
            Command::Fragment { program, .. } => program.fingerprint().hash(hash),
            Command::Fill {
                bounds: b,
                radius,
                color,
            } => {
                bounds(b, hash);
                floats(&[*radius], hash);
                color.bgra().hash(hash);
            }
            Command::Outline {
                bounds: b,
                radius,
                thickness,
                color,
            } => {
                bounds(b, hash);
                floats(&[*radius, *thickness], hash);
                color.bgra().hash(hash);
            }
            Command::Ring {
                center,
                radius,
                thickness,
                color,
            } => {
                floats(&[center.0, center.1, *radius, *thickness], hash);
                color.bgra().hash(hash);
            }
            Command::Text {
                bounds: b,
                size,
                text,
                font,
                color,
                align,
            } => {
                bounds(b, hash);
                floats(&[*size], hash);
                text.hash(hash);
                font.id().hash(hash);
                color.bgra().hash(hash);
                std::mem::discriminant(align).hash(hash);
            }
            Command::InsetShadow {
                bounds: b,
                radius,
                blur,
                spread,
                color,
            } => {
                bounds(b, hash);
                floats(&[*radius, *blur, *spread], hash);
                color.bgra().hash(hash);
            }
            Command::Image {
                bounds: b,
                image,
                fit,
            } => {
                bounds(b, hash);
                image.id().hash(hash);
                std::mem::discriminant(fit).hash(hash);
            }
            Command::Clip {
                bounds: b,
                commands,
            } => {
                bounds(b, hash);
                block(commands, hash);
            }
            Command::Translate { offset, commands } => {
                floats(&[offset.0, offset.1], hash);
                block(commands, hash);
            }
            Command::Line {
                from,
                to,
                thickness,
                color,
            } => {
                floats(&[from.0, from.1, to.0, to.1, *thickness], hash);
                color.bgra().hash(hash);
            }
            Command::Polygon { points, color } => {
                points.len().hash(hash);
                for point in points {
                    floats(point, hash);
                }
                color.bgra().hash(hash);
            }
            Command::External { bounds: b, key } => {
                bounds(b, hash);
                key.hash(hash);
            }
        }
    }
}
