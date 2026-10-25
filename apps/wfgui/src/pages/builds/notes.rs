use super::*;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Default)]
struct Block {
    spans: Vec<Span>,
    heading: u8,
    rule: bool,
}
struct Span {
    text: String,
    bold: bool,
    link: Option<String>,
}

#[derive(Default)]
pub struct Notes {
    source: String,
    blocks: Vec<Block>,
}
impl Notes {
    pub fn render(&mut self, ui: &mut Ui, source: &str, width: f32, mut y: f32) -> f32 {
        if self.source != source {
            self.source = source.into();
            self.blocks = parse(source);
        }
        for block in &self.blocks {
            if block.rule {
                ui.fill(ui.rect(16.0, y, width - 32.0, 1.0), 0.0, MUTED);
                y += 14.0;
                continue;
            }
            let size = match block.heading {
                1 => 24.0,
                2 => 20.0,
                3.. => 17.0,
                _ => 14.0,
            };
            let line = size * 1.45;
            let mut x = 16.0;
            for span in &block.spans {
                for (row, paragraph) in span.text.split('\n').enumerate() {
                    if row > 0 {
                        x = 16.0;
                        y += line;
                    }
                    for word in paragraph.split_inclusive(char::is_whitespace) {
                        let bold = span.bold || block.heading > 0;
                        let word_width = ui.measure(word, size, bold);
                        if x + word_width > width - 16.0 && x > 16.0 {
                            x = 16.0;
                            y += line;
                        }
                        let color = if span.link.is_some() {
                            rgb(0xbfa0f0)
                        } else {
                            TEXT
                        };
                        let rect = ui.rect(x, y, word_width.min(width - 32.0) + 1.0, line);
                        if bold {
                            ui.bold(rect, word, size, color);
                        } else {
                            ui.text(rect, word, size, color);
                        }
                        if let Some(url) = &span.link {
                            ui.hit(format!("build:link:{url}"), rect, url, true);
                        }
                        x += word_width;
                    }
                }
            }
            y += line + 10.0;
        }
        y
    }
}

fn parse(source: &str) -> Vec<Block> {
    let mut blocks = vec![];
    let mut current = Block::default();
    let mut bold = 0usize;
    let mut link = None;
    for event in Parser::new(source) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => current.heading = level as u8,
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => link = link_url(&dest_url),
            Event::End(TagEnd::Link) => link = None,
            Event::Start(Tag::Item) => current.spans.push(Span {
                text: "- ".into(),
                bold: false,
                link: None,
            }),
            Event::Text(text) | Event::Code(text) => current.spans.push(Span {
                text: text.into_string(),
                bold: bold > 0,
                link: link.clone(),
            }),
            Event::SoftBreak => current.spans.push(Span {
                text: " ".into(),
                bold: false,
                link: None,
            }),
            Event::HardBreak => current.spans.push(Span {
                text: "\n".into(),
                bold: false,
                link: None,
            }),
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock,
            ) => {
                if !current.spans.is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
            }
            Event::Rule => {
                blocks.push(Block {
                    rule: true,
                    ..Default::default()
                });
            }
            _ => (),
        }
    }
    if !current.spans.is_empty() {
        blocks.push(current);
    }
    blocks
}
pub fn link_url(url: &str) -> Option<String> {
    if (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_control)
    {
        Some(url.into())
    } else if url.starts_with('/') && !url.starts_with("//") && !url.chars().any(char::is_control) {
        Some(format!("https://overframe.gg{url}"))
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_parse_blocks_and_never_turn_script_urls_into_actions() {
        let blocks = parse(
            "# Overview\n\nUse **strength** and [notes](/build/1).\n\n[bad](javascript:alert)\n\n- Mod",
        );
        assert_eq!(blocks[0].heading, 1);
        assert!(
            blocks[1]
                .spans
                .iter()
                .any(|s| s.bold && s.text == "strength")
        );
        assert!(
            blocks[1]
                .spans
                .iter()
                .any(|s| s.link.as_deref() == Some("https://overframe.gg/build/1"))
        );
        assert!(blocks[2].spans.iter().all(|s| s.link.is_none()));
        assert!(link_url("//attacker.test").is_none());
    }
}
