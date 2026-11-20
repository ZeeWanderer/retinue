use super::Shell;
use crate::data::foundry::Asset;
use retinue_render::thumbnails::Request;
use std::collections::HashSet;

pub(super) struct Requests {
    pub assets: Vec<Asset>,
    pub images: Vec<Request>,
}

pub(super) fn requests(view: &Shell) -> Requests {
    let mut assets = view.foundry.desired_assets();
    assets.extend(view.pages.assets());
    let mut seen = HashSet::new();
    assets.retain(|asset| seen.insert(asset.id.clone()));
    let mut images = view.foundry.image_requests();
    images.extend(view.pages.image_requests());
    let mut seen = HashSet::new();
    images.retain(|image| seen.insert(image.key));
    Requests { assets, images }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{foundry, storage::Snapshot},
        presentation::shell::Page,
    };

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
        shell.foundry.assets(vec![descriptor.clone()]);
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
        shell.foundry.assets(vec![descriptor.clone()]);
        assert!(keys(&shell).contains(&descriptor.key(126).unwrap()));
        shell.set_page(Page::Settings);
        assert!(!requests(&shell).assets.iter().any(|a| a.id == "shared-art"));
        shell.set_page(Page::Foundry);
        assert!(requests(&shell).assets.iter().any(|a| a.id == "shared-art"));
    }
}
