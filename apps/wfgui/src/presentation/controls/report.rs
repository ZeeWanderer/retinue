use retinue_render::{Bounds, Color, Command};
use retinue_ui::Rect;
use serde_json::{Value, json};

fn color(c: Color) -> String {
    let c = c.bgra();
    format!("#{:02x}{:02x}{:02x}{:02x}", c[2], c[1], c[0], c[3])
}

fn project(b: Bounds, origin: (i32, i32), clip: Rect, scale: f32) -> Option<Rect> {
    let x = origin.0 + (b.x * scale).round() as i32;
    let y = origin.1 + (b.y * scale).round() as i32;
    let right = (x as i64 + (b.width * scale).round() as i64).min((clip.x + clip.width) as i64);
    let bottom = (y as i64 + (b.height * scale).round() as i64).min((clip.y + clip.height) as i64);
    let left = (x as i64).max(clip.x as i64);
    let top = (y as i64).max(clip.y as i64);
    (right > left && bottom > top).then_some(Rect {
        x: left as u32,
        y: top as u32,
        width: (right - left).max(0) as u32,
        height: (bottom - top).max(0) as u32,
    })
}

pub(super) fn commands(
    out: &mut Vec<Value>,
    items: &[Command],
    origin: (i32, i32),
    clip: Rect,
    scale: f32,
) {
    for item in items {
        let (bounds, mut value) = match item {
            Command::Translate {
                offset,
                commands: children,
            } => {
                commands(
                    out,
                    children,
                    (
                        origin.0 + (offset.0 * scale).round() as i32,
                        origin.1 + (offset.1 * scale).round() as i32,
                    ),
                    clip,
                    scale,
                );
                continue;
            }
            Command::Clip {
                bounds,
                commands: children,
            } => {
                if let Some(clip) = project(*bounds, origin, clip, scale) {
                    commands(out, children, origin, clip, scale);
                }
                continue;
            }
            Command::Fragment { program, .. } => {
                commands(out, program.commands(), origin, clip, scale);
                continue;
            }
            Command::Fill {
                bounds,
                radius,
                color: c,
            } => (
                *bounds,
                json!({"kind":"fill","radius":radius,"color":color(*c)}),
            ),
            Command::Outline {
                bounds,
                radius,
                color: c,
                ..
            } => (
                *bounds,
                json!({"kind":"outline","radius":radius,"color":color(*c)}),
            ),
            Command::Text {
                bounds,
                text,
                size,
                color: c,
                font,
                ..
            } => (
                *bounds,
                json!({"kind":"text","text":text,"font_size":size,"font_id":format!("{:?}",font.id()),"color":color(*c)}),
            ),
            Command::Image { bounds, .. } => (*bounds, json!({"kind":"image"})),
            Command::InsetShadow {
                bounds,
                radius,
                blur,
                spread,
                color: c,
            } => (
                *bounds,
                json!({"kind":"inset_shadow","radius":radius,"blur":blur,"spread":spread,"color":color(*c)}),
            ),
            _ => continue,
        };
        if let Some(r) = project(bounds, origin, clip, scale) {
            value["bounds"] = json!({"x":r.x as f32/scale,"y":r.y as f32/scale,"width":r.width as f32/scale,"height":r.height as f32/scale});
            out.push(value);
        }
    }
}
