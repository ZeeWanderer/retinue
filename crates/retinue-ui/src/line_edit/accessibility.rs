use super::{LineEdit, Rect};
use accesskit::{Action, Node, NodeId, Role, TextDirection, TextPosition, TextSelection};
use parley::{editing::Cursor, layout::Affinity};

pub(super) struct Span {
    id: NodeId,
    bytes: Vec<usize>,
}

impl LineEdit {
    /// Publish selectable text from the displayed single-line layout. The host
    /// supplies unused node IDs in its accessibility tree's namespace.
    pub fn accessible_text(
        &mut self,
        parent: &mut Node,
        out: &mut Vec<(NodeId, Node)>,
        field: Rect,
        size: f32,
        scale: f32,
        mut allocate: impl FnMut() -> NodeId,
    ) {
        if self.is_password() {
            return;
        }
        parent.set_value(self.text());
        if self.composition.is_some() || field.is_empty() {
            self.access_runs.clear();
            return;
        }
        self.layout_scaled(field, size, scale);
        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        let text = self.editor.raw_text();
        let x = field.x as f32 - self.scroll * scale;
        let y = field.y as f32 + (field.height as f32 - layout.height() * scale) * 0.5;
        let mut previous = std::mem::take(&mut self.access_runs).into_iter();
        let mut spans = Vec::new();
        let start = out.len();
        for line in layout.lines() {
            let m = line.metrics();
            let mut offset = m.offset;
            let mut runs: Vec<_> = line
                .runs()
                .map(|r| {
                    let left = offset;
                    offset += r.advance();
                    (r, left)
                })
                .collect();
            runs.sort_by_key(|(r, _)| r.text_range().start);
            for (run, left) in runs {
                let mut clusters = run.clusters().peekable();
                let rtl = run.is_rtl();
                let mut consumed = 0.0;
                loop {
                    let first = clusters
                        .peek()
                        .map_or(run.text_range().start, |c| c.text_range().start)
                        .min(text.len());
                    let mut bytes = vec![first];
                    let (mut lengths, mut positions, mut widths, mut words) =
                        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
                    let mut width = 0.0;
                    for cluster in clusters.by_ref().take(256) {
                        let range = cluster.text_range();
                        if range.is_empty() || text.is_empty() {
                            continue;
                        }
                        let Ok(len) = u8::try_from(range.len()) else {
                            // AccessKit cannot encode this indivisible cluster. Do not
                            // publish truncated byte offsets or invent caret positions.
                            out.truncate(start);
                            return;
                        };
                        if cluster.is_word_boundary() && !cluster.is_space_or_nbsp() {
                            words.push(lengths.len() as u8);
                        }
                        lengths.push(len);
                        positions.push(width * scale);
                        widths.push(cluster.advance() * scale);
                        bytes.push(range.end);
                        width += cluster.advance();
                    }
                    let id = previous
                        .next()
                        .filter(|s| s.bytes == bytes)
                        .map_or_else(&mut allocate, |s| s.id);
                    let mut node = Node::new(Role::TextRun);
                    let dx = if rtl {
                        left + run.advance() - consumed - width
                    } else {
                        left + consumed
                    };
                    node.set_bounds(accesskit::Rect {
                        x0: (x + dx * scale) as f64,
                        x1: (x + (dx + width) * scale) as f64,
                        y0: (y + m.content_block_min_coord * scale) as f64,
                        y1: (y + m.content_block_max_coord * scale) as f64,
                    });
                    node.set_value(&text[first..*bytes.last().unwrap()]);
                    node.set_text_direction(if rtl {
                        TextDirection::RightToLeft
                    } else {
                        TextDirection::LeftToRight
                    });
                    node.set_character_lengths(lengths);
                    node.set_character_positions(positions);
                    node.set_character_widths(widths);
                    node.set_word_starts(words);
                    spans.push(Span { id, bytes });
                    out.push((id, node));
                    consumed += width;
                    if clusters.peek().is_none() {
                        break;
                    }
                }
            }
        }
        for (i, (id, node)) in out[start..].iter_mut().enumerate() {
            parent.push_child(*id);
            if i > 0 {
                node.set_previous_on_line(spans[i - 1].id);
            }
            if let Some(next) = spans.get(i + 1) {
                node.set_next_on_line(next.id);
            }
        }
        let selection = self.editor.raw_selection();
        let position = |cursor: Cursor| {
            let mut boundary = None;
            for span in &spans {
                if let Ok(index) = span.bytes.binary_search(&cursor.index()) {
                    let p = TextPosition {
                        node: span.id,
                        character_index: index,
                    };
                    if (index != 0 || cursor.affinity() == Affinity::Downstream)
                        && (index + 1 < span.bytes.len() || cursor.affinity() == Affinity::Upstream)
                    {
                        return Some(p);
                    }
                    boundary = Some(p);
                }
            }
            boundary
        };
        if let (Some(anchor), Some(focus)) =
            (position(selection.anchor()), position(selection.focus()))
        {
            parent.set_text_selection(TextSelection { anchor, focus });
            parent.add_action(Action::SetTextSelection);
        }
        parent.set_clips_children();
        self.access_runs = spans;
    }

    pub fn select_accessible_text(&mut self, selection: &TextSelection) -> bool {
        if self.is_password() || self.composition.is_some() {
            return false;
        }
        let endpoint = |p: TextPosition| {
            let span = self.access_runs.iter().find(|s| s.id == p.node)?;
            let byte = *span.bytes.get(p.character_index)?;
            let affinity = if p.character_index + 1 == span.bytes.len() {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            };
            Some((byte, affinity))
        };
        let (Some(anchor), Some(focus)) = (endpoint(selection.anchor), endpoint(selection.focus))
        else {
            return false;
        };
        self.edit(|d| {
            let layout = d.layout();
            let selection = parley::editing::Selection::new(
                Cursor::from_byte_index(layout, anchor.0, anchor.1),
                Cursor::from_byte_index(layout, focus.0, focus.1),
            );
            d.set_selection(selection);
        });
        self.changed(false);
        true
    }
}
