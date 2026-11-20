use super::*;
use crate::data::foundry::Descriptor;
use crate::presentation::widgets::AssetImage;
use retinue_render::{painter::RasterImage, resources::ResourceCache, thumbnails::Key};
use retinue_ui::{authoring::*, environment::Environment, ir::InstanceGraph};

fn asset() -> Asset {
    Asset {
        id: "test.asset".into(),
        source: "wfcd".into(),
        image_name: "test.png".into(),
    }
}

fn descriptor(digest: &str) -> Descriptor {
    Descriptor {
        id: asset().id,
        path: "unused.png".into(),
        digest: digest.repeat(32),
    }
}

fn pixels(
    cache: &mut ResourceCache<Key, RasterImage>,
    request: &Request,
    color: u8,
) -> super::super::Resource<RasterImage> {
    cache.get_or_insert_with(request.key, || {
        RasterImage::render(32, 32, |p| {
            p.fill_rounded_rect(0, 0, 32, 32, 0, [color, 40, 80, 255]);
        })
        .unwrap()
    })
}

#[test]
fn image_demand_is_shared_and_released_with_the_view() {
    let store = ImageStore::default();
    let source = Source::Asset(asset());
    let first = store.lease(&source, 32);
    let second = store.lease(&source, 32);
    assert!(Rc::ptr_eq(&first, &second));
    assert_eq!(store.assets().len(), 1);
    drop(first);
    assert_eq!(store.assets().len(), 1);
    drop(second);
    assert!(store.assets().is_empty());
    assert!(store.0.borrow().leases.is_empty());
}

#[test]
fn asynchronous_renditions_update_only_image_content_and_keep_stale_pixels() {
    let store = ImageStore::default();
    let mut tree = InstanceGraph::default();
    let environment = Environment::default();
    environment.provide(store.clone());
    let root = tree
        .mount_body(
            |_| {
                compose(AssetImage::new(Some(&asset()), 32))
                    .frame(Frame::new().width(80.0).height(80.0))
            },
            environment,
        )
        .unwrap();
    tree.set_root(Some(root)).unwrap();
    tree.advance((100, 100), 1.0, None).unwrap();
    let mut media = Media::default();
    assert_eq!(store.assets().len(), 1);
    assert!(store.requests(&media).is_empty());
    assert!(!store.publish(&media));
    media.assets(vec![descriptor("ab")]);
    let first = store.requests(&media);
    let mut cache = ResourceCache::new(1024 * 1024);
    let image = pixels(&mut cache, &first[0], 130);
    media.accept(&[(first[0].key, image)], &first);
    assert!(store.publish(&media));
    assert!(tree.has_pending_inputs());
    let update = tree.advance((100, 100), 1.0, None).unwrap();
    assert_ne!(
        update.completion.unwrap(),
        std::task::Poll::Ready(retinue_render::damage::Damage::None)
    );
    assert_eq!(update.work.instances_mounted, 0);
    assert!(!store.publish(&media));
    assert!(!tree.has_pending_inputs());

    let lease = store.active().pop().unwrap();
    let previous = lease.image.pixels().unwrap();
    media.assets(vec![descriptor("cd")]);
    let fresh = store.requests(&media);
    media.accept(&[], &fresh);
    assert!(!store.publish(&media));
    assert!(lease.image.pixels().as_ref() == Some(&previous));
    assert!(!lease.image.is_fresh());
    let image = pixels(&mut cache, &fresh[0], 220);
    media.accept(&[(fresh[0].key, image)], &fresh);
    assert!(store.publish(&media));
    assert!(lease.image.pixels().as_ref() != Some(&previous));
    assert!(lease.image.is_fresh());
    assert_eq!(
        tree.advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .instances_mounted,
        0
    );
    drop(lease);
    drop(tree);
    assert!(store.requests(&media).is_empty());
}
