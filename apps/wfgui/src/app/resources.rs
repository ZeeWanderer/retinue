use super::Shell;
use crate::data::foundry::{Asset, Descriptor};
use retinue_render::thumbnails::{Key, Request, parse_digest};
use std::collections::HashSet;

pub(super) struct Requests {
    pub assets: Vec<Asset>,
    pub images: Vec<Request>,
}

pub(super) fn requests(view: &Shell) -> Requests {
    Requests {
        assets: assets(view),
        images: images(view),
    }
}

pub(super) fn assets(view: &Shell) -> Vec<Asset> {
    let mut assets = view.foundry.desired_assets();
    assets.extend(view.pages.assets());
    let mut seen = HashSet::new();
    assets.retain(|asset| seen.insert(asset.id.clone()));
    assets
}

pub(super) fn images(view: &Shell) -> Vec<Request> {
    let mut images = view.foundry.image_requests();
    images.extend(view.pages.image_requests());
    let mut seen = HashSet::new();
    images.retain(|image| seen.insert(image.key));
    images
}

#[derive(Default)]
pub(super) struct Publication {
    pub changed: bool,
    pub retry_submitted: bool,
}

pub(super) fn publish_descriptors(
    view: &mut Shell,
    mut descriptors: Vec<Descriptor>,
    submitted: &[Key],
) -> Publication {
    use crate::domain::media::DescriptorUpdate;
    let mut result = Publication::default();
    descriptors.retain(|d| {
        match view.pages.publish_descriptor(d) {
            DescriptorUpdate::Unchanged => return view.foundry.asset_errors.contains_key(&d.id),
            DescriptorUpdate::RetrySameDigest => {
                if let Some(digest) = parse_digest(&d.digest) {
                    result.retry_submitted |= submitted.iter().any(|key| key.digest == digest);
                }
            }
            DescriptorUpdate::Changed => (),
        }
        true
    });
    result.changed = !descriptors.is_empty();
    if result.changed {
        view.foundry.assets(descriptors);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{foundry, storage::Snapshot},
        presentation::shell::Page,
    };

    #[test]
    fn repeated_descriptor_delivery_does_not_restart_visible_thumbnails() {
        let mut shell = Shell::new(Some(Snapshot::fixture()), 1.0).unwrap();
        let mut d = Descriptor {
            id: "art".into(),
            path: "first.png".into(),
            digest: "ab".repeat(32),
        };
        let key = d.key(105).unwrap();
        let publish = |shell: &mut Shell, d: &Descriptor, submitted: &[Key]| {
            publish_descriptors(shell, vec![d.clone()], submitted)
        };
        let first = publish(&mut shell, &d, &[]);
        assert!(first.changed && !first.retry_submitted);
        let duplicate = publish(&mut shell, &d, &[key]);
        assert!(!duplicate.changed && !duplicate.retry_submitted);
        d.path = "relocated.png".into();
        let moved = publish(&mut shell, &d, &[key]);
        assert!(moved.changed && moved.retry_submitted);
        shell
            .pages
            .media
            .image_health(&[], &[(key, "missing".into())]);
        let retry = publish(&mut shell, &d, &[key]);
        assert!(retry.changed && retry.retry_submitted);
        let offscreen = publish(&mut shell, &d, &[]);
        assert!(offscreen.changed && !offscreen.retry_submitted);
        d.digest = "cd".repeat(32);
        let replaced = publish(&mut shell, &d, &[key]);
        assert!(replaced.changed && !replaced.retry_submitted);
    }

    #[test]
    fn requests_follow_visibility_scale_and_late_descriptors() {
        let mut shell = Shell::new(Some(Snapshot::fixture()), 1.0).unwrap();
        shell.resize((1440, 900), 1.0).unwrap();
        shell.set_page(Page::Foundry);
        let mut fixture = foundry::fixture();
        for card in &mut fixture.items {
            card.asset = Some(Asset {
                id: "shared-art".into(),
                ..Default::default()
            });
        }
        shell.foundry.update(fixture);
        shell.prepare_paint();
        assert_eq!(
            requests(&shell)
                .assets
                .iter()
                .filter(|a| a.id == "shared-art")
                .count(),
            1
        );
        let mut descriptor = foundry::Descriptor {
            id: "shared-art".into(),
            path: "not-read.png".into(),
            digest: "ab".repeat(32),
        };
        publish_descriptors(&mut shell, vec![descriptor.clone()], &[]);
        let page_asset = Asset {
            id: descriptor.id.clone(),
            ..Default::default()
        };
        assert_eq!(
            shell.pages.media.request(&page_asset, 105).unwrap().key,
            descriptor.key(105).unwrap()
        );
        let keys = |shell: &Shell| {
            requests(shell)
                .images
                .into_iter()
                .map(|r| r.key)
                .collect::<Vec<_>>()
        };
        let first = keys(&shell);
        assert!(first.contains(&descriptor.key(105).unwrap()));
        assert_eq!(first.iter().collect::<HashSet<_>>().len(), first.len());
        shell.foundry.resize((1000, 800), 1.2);
        assert!(keys(&shell).contains(&descriptor.key(126).unwrap()));
        descriptor.digest = "cd".repeat(32);
        publish_descriptors(&mut shell, vec![descriptor.clone()], &[]);
        assert_eq!(
            shell.pages.media.request(&page_asset, 126).unwrap().key,
            descriptor.key(126).unwrap()
        );
        assert!(keys(&shell).contains(&descriptor.key(126).unwrap()));
        shell.set_page(Page::Settings);
        assert!(!requests(&shell).assets.iter().any(|a| a.id == "shared-art"));
        shell.set_page(Page::Foundry);
        assert!(requests(&shell).assets.iter().any(|a| a.id == "shared-art"));
    }
}
