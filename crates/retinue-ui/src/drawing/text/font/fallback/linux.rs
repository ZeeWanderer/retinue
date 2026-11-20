use super::{covers, required};
use fontconfig::{CharSet, Fontconfig, ObjectSet, Pattern, UnicodeCoverage};
use icu_properties::{
    CodePointMapData, PropertyNamesShort,
    props::{IndicSyllabicCategory, JoiningType, Script},
};
use parley::fontique::{FontInfo, FontStyle, SourceCache, SourceId, SourceInfo, SourceKind};
use retinue_render::text::{Font, FontData};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    ffi::CString,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, OnceLock, Weak},
};

type Face = (PathBuf, u32);
static CONFIG: OnceLock<Option<Fontconfig>> = OnceLock::new();
static FACES: LazyLock<Mutex<HashMap<Face, Weak<Font>>>> = LazyLock::new(Mutex::default);

pub(crate) fn prepare_host_fonts(
    host: &retinue_render::RenderHost,
) -> Option<retinue_render::work::Submission> {
    use retinue_render::work::{Graph, Priority};
    if CONFIG.get().is_some() || host.task_threads(2.try_into().unwrap()).get() == 1 {
        return None;
    }
    let mut graph = Graph::default();
    graph.add(&[], std::num::NonZeroUsize::MIN, |_| {
        CONFIG.get_or_init(Fontconfig::new);
        Ok(())
    });
    host.executor()
        .submit(graph, Priority::Background, || {})
        .ok()
}

struct Match {
    family: String,
    cluster: String,
    style: [i32; 3],
    font: Weak<Font>,
}

static MATCHES: Mutex<VecDeque<Match>> = Mutex::new(VecDeque::new());

fn face(pattern: &Pattern<'_>) -> Option<Face> {
    Some((
        pattern.filename().ok()?.into(),
        pattern.face_index().ok()?.try_into().ok()?,
    ))
}

fn load(key: Face) -> Option<Arc<Font>> {
    if let Some(font) = FACES.lock().unwrap().get(&key).and_then(Weak::upgrade) {
        return Some(font);
    }
    let source = SourceInfo::new(SourceId::new(), SourceKind::Path(key.0.as_path().into()));
    let bytes = SourceCache::default().get(&source)?;
    let font = Arc::new(Font::new(FontData::new(bytes, key.1)).ok()?);
    let mut cache = FACES.lock().unwrap();
    cache.retain(|_, weak| weak.strong_count() != 0);
    if let Some(existing) = cache.get(&key).and_then(Weak::upgrade) {
        return Some(existing);
    }
    cache.insert(key, Arc::downgrade(&font));
    Some(font)
}

pub(super) fn contextual_script(cluster: &str) -> Option<[u8; 4]> {
    let c = required(cluster).find(|&c| {
        !matches!(
            CodePointMapData::<JoiningType>::new().get(c),
            JoiningType::NonJoining | JoiningType::Transparent
        ) || CodePointMapData::<IndicSyllabicCategory>::new().get(c) != IndicSyllabicCategory::Other
    })?;
    let script = CodePointMapData::<Script>::new().get(c);
    if matches!(script, Script::Common | Script::Inherited | Script::Unknown) {
        return None;
    }
    PropertyNamesShort::<Script>::new()
        .get_locale_script(script)?
        .as_str()
        .as_bytes()
        .try_into()
        .ok()
}

pub(super) fn select(
    info: &FontInfo,
    family: &str,
    cluster: &str,
    script: Option<[u8; 4]>,
    mut accept: impl FnMut(Arc<Font>) -> Option<usize>,
) -> Option<usize> {
    let weight = match info.weight().value() as u16 {
        0..=349 => fontconfig::FC_WEIGHT_LIGHT,
        350..=449 => fontconfig::FC_WEIGHT_REGULAR,
        450..=549 => fontconfig::FC_WEIGHT_MEDIUM,
        550..=649 => fontconfig::FC_WEIGHT_SEMIBOLD,
        _ => fontconfig::FC_WEIGHT_BOLD,
    };
    let slant = match info.style() {
        FontStyle::Normal => fontconfig::FC_SLANT_ROMAN,
        FontStyle::Italic => fontconfig::FC_SLANT_ITALIC,
        FontStyle::Oblique(_) => fontconfig::FC_SLANT_OBLIQUE,
    };
    let width = info.width().percentage().round() as i32;
    let style = [weight, slant, width];
    let cached = MATCHES.lock().unwrap().iter().find_map(|m| {
        (m.style == style && m.family == family && m.cluster == cluster)
            .then(|| m.font.upgrade())
            .flatten()
    });
    if let Some(font) = cached
        && let Some(i) = accept(font)
    {
        return Some(i);
    }
    let fc = CONFIG.get_or_init(Fontconfig::new).as_ref()?;
    let mut pattern = Pattern::new(fc).ok()?;
    let mut charset = CharSet::new(fc).ok()?;
    for c in required(cluster) {
        charset.add_char(c).ok()?;
    }
    pattern.add_charset(charset).ok()?;
    let mut fields = ObjectSet::new(fc).ok()?;
    fields.add(fontconfig::FC_FILE).ok()?;
    fields.add(fontconfig::FC_INDEX).ok()?;
    let candidates = fontconfig::list_fonts(&pattern, Some(&fields)).ok()?;
    let mut candidates: BTreeSet<_> = candidates.iter().filter_map(|p| face(&p)).collect();
    if candidates.is_empty() {
        return None;
    }
    pattern
        .add_string(fontconfig::FC_FAMILY, &CString::new(family).ok()?)
        .ok()?;
    pattern.add_integer(fontconfig::FC_WEIGHT, weight).ok()?;
    pattern.add_integer(fontconfig::FC_SLANT, slant).ok()?;
    pattern.add_integer(fontconfig::FC_WIDTH, width).ok()?;
    let sorted = pattern.sort_fonts(UnicodeCoverage::NoTrim).ok()?;
    let mut ordered: Vec<_> = sorted
        .iter()
        .filter_map(|p| face(&p))
        .filter(|key| candidates.remove(key))
        .collect();
    // Sorting may omit redundant Unicode coverage even when only a later
    // face has a renderable outline or the script's contextual forms.
    ordered.extend(candidates);
    let mut coverage_only = Vec::new();
    let mut first = true;
    let mut accept = |font: Arc<Font>| {
        // A lower-ranked face accepted only after an owner-specific registration
        // failure is not the best match for other owners.
        let weak = Arc::downgrade(&font);
        let result = accept(font);
        if first && result.is_some() && cluster.len() <= 64 {
            let mut cache = MATCHES.lock().unwrap();
            cache.retain(|m| {
                m.font.strong_count() != 0
                    && !(m.style == style && m.family == family && m.cluster == cluster)
            });
            if cache.len() == 128 {
                cache.pop_front();
            }
            cache.push_back(Match {
                family: family.into(),
                cluster: cluster.into(),
                style,
                font: weak,
            });
        }
        first = false;
        result
    };
    for key in ordered {
        let Some(font) = load(key.clone()).filter(|font| covers(font, cluster)) else {
            continue;
        };
        if script.is_some_and(|script| !font.has_script_substitutions(script)) {
            coverage_only.push(key);
        } else if let Some(i) = accept(font) {
            return Some(i);
        }
    }
    coverage_only.into_iter().filter_map(load).find_map(accept)
}

#[cfg(test)]
mod tests {
    use super::super::super::TextFace;
    use super::*;
    use retinue_render::painter::CachedFont;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "fresh-process host font selection and registration timings"]
    fn host_font_stage_costs() {
        assert!(!cfg!(debug_assertions), "use a release build");
        let bytes = include_bytes!("../../../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let make_face = || {
            let font = CachedFont::from_bytes(bytes).unwrap();
            TextFace::for_font(&font).unwrap()
        };
        let started = Instant::now();
        assert!(CONFIG.get_or_init(Fontconfig::new).is_some());
        println!("config_us={}", started.elapsed().as_micros());
        let measure = |name, face: &TextFace| {
            for cluster in ["漢", "س", "😀"] {
                if covers(&face.native, cluster) {
                    continue;
                }
                let mut registration = Duration::ZERO;
                let start = Instant::now();
                let result = select(
                    &face.info,
                    &face.family,
                    cluster,
                    contextual_script(cluster),
                    |font| {
                        let started = Instant::now();
                        let result = face.fallbacks.borrow_mut().register(face, font);
                        registration += started.elapsed();
                        result
                    },
                );
                assert!(
                    result.is_some(),
                    "fixture requires outline coverage for {cluster}"
                );
                println!(
                    "{name} {cluster} select_us={} register_us={}",
                    (start.elapsed() - registration).as_micros(),
                    registration.as_micros()
                );
            }
        };
        let cold = make_face();
        measure("first", &cold);
        std::thread::scope(|s| s.spawn(|| measure("peer", &make_face())).join().unwrap());
    }
}
