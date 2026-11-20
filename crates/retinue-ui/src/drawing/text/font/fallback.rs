use super::*;
use unicode_segmentation::UnicodeSegmentation;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::prepare_host_fonts;

#[derive(Default)]
pub(super) struct Fallbacks {
    faces: Vec<(String, Arc<Font>)>,
    matches: VecDeque<(String, Option<usize>)>,
}

fn required(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars().filter(|c| !c.is_control() && !matches!(*c, '\u{200c}' | '\u{200d}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}'))
}

pub(super) fn covers(font: &Font, text: &str) -> bool {
    required(text).all(|c| font.contains(c))
}

impl Fallbacks {
    pub fn get(&self, id: (u64, u32)) -> Option<&Font> {
        self.faces
            .iter()
            .find(|(_, f)| f.identity() == id)
            .map(|(_, f)| f.as_ref())
    }

    pub fn families<'a>(&mut self, face: &'a TextFace, text: &str) -> FontFamily<'a> {
        let mut chosen: Vec<usize> = Vec::new();
        let mut contextual = 0;
        for cluster in text.graphemes(true) {
            if (face.ascii_covered && cluster.is_ascii()) || covers(&face.native, cluster) {
                continue;
            }
            #[cfg(target_os = "linux")]
            let script = linux::contextual_script(cluster);
            #[cfg(not(target_os = "linux"))]
            let script: Option<[u8; 4]> = None;
            let eligible = if script.is_some() {
                &chosen[..contextual]
            } else {
                &chosen
            };
            if eligible.iter().any(|&i| {
                let font = &self.faces[i].1;
                covers(font, cluster) && script.is_none_or(|tag| font.has_script_substitutions(tag))
            }) {
                continue;
            }
            let index = if let Some((_, result)) = self.matches.iter().find(|(s, _)| s == cluster) {
                *result
            } else {
                #[cfg(target_os = "linux")]
                let result = linux::select(&face.info, &face.family, cluster, script, |font| {
                    self.register(face, font)
                });
                #[cfg(not(target_os = "linux"))]
                let result = None;
                if cluster.len() <= 64 {
                    if self.matches.len() == 128 {
                        self.matches.pop_front();
                    }
                    self.matches.push_back((cluster.into(), result));
                }
                result
            };
            if let Some(i) = index {
                if script.is_some_and(|tag| self.faces[i].1.has_script_substitutions(tag)) {
                    // An earlier symbol font may contain the letters but not their joining forms.
                    if !chosen[..contextual].contains(&i) {
                        if let Some(at) = chosen.iter().position(|&value| value == i) {
                            chosen.remove(at);
                        }
                        chosen.insert(0, i);
                        contextual += 1;
                    }
                } else if !chosen.contains(&i) {
                    chosen.push(i);
                }
            }
        }
        if chosen.is_empty() {
            return FontFamily::named(&face.family);
        }
        FontFamily::List(
            std::iter::once(parley::FontFamilyName::named(&face.family))
                .chain(
                    chosen
                        .iter()
                        .map(|&i| parley::FontFamilyName::named(&self.faces[i].0).into_owned()),
                )
                .collect::<Vec<_>>()
                .into(),
        )
    }

    #[cfg(test)]
    pub fn use_for_test(&mut self, face: &TextFace, cluster: &str, font: Font) {
        let i = self.register(face, Arc::new(font)).unwrap();
        self.matches.push_back((cluster.into(), Some(i)));
    }

    fn register(&mut self, face: &TextFace, candidate: Arc<Font>) -> Option<usize> {
        use parley::fontique::FontInfoOverride;
        if let Some(i) = self
            .faces
            .iter()
            .position(|(_, f)| f.identity() == candidate.identity())
        {
            return Some(i);
        }
        let mut fonts = face.fonts.borrow_mut();
        let mut next = fonts.collection.clone();
        let info = FontInfoOverride {
            weight: Some(face.info.weight()),
            style: Some(face.info.style()),
            width: Some(face.info.width()),
            ..Default::default()
        };
        let mut families = next.register_fonts(candidate.data().data.clone(), Some(info));
        if families.iter().any(|(id, _)| {
            next.family_name(*id)
                .is_some_and(|n| fonts.collection.family_id(n).is_some())
        }) {
            let (blob, index) = candidate.identity();
            let name = format!("RetinueFallback-{blob}-{index}");
            if fonts.collection.family_id(&name).is_some() {
                return None;
            }
            next = fonts.collection.clone();
            families = next.register_fonts(
                candidate.data().data.clone(),
                Some(FontInfoOverride {
                    family_name: Some(&name),
                    ..info
                }),
            );
        }
        let (id, _) = families.into_iter().find(|(_, entries)| {
            entries
                .iter()
                .any(|entry| entry.index() == candidate.data().index)
        })?;
        // A collection can contain several faces. Never change the selected
        // primary family or silently register a different collection index.
        let name = next.family_name(id)?.to_owned();
        let mut query = next.query(&mut fonts.source_cache);
        query.set_families([id]);
        let exact = query
            .first_available_font()
            .is_some_and(|selected| (selected.blob.id(), selected.index) == candidate.identity());
        drop(query);
        if !exact {
            return None;
        }
        fonts.collection = next;
        self.faces.push((name, candidate));
        Some(self.faces.len() - 1)
    }
}
