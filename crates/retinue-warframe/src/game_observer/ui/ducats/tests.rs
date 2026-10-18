use super::*;

fn tree(paths: &[(&str, Option<&str>, Option<[f32; 4]>)]) -> DisplayTree {
    let mut nodes = Vec::<DisplayNode>::new();
    let mut ids = BTreeMap::<String, u64>::new();
    for (path, text, bounds) in paths {
        let mut parent = 0;
        let parts: Vec<_> = path.split('.').collect();
        for end in 1..=parts.len() {
            let key = parts[..end].join(".");
            let address = if let Some(id) = ids.get(&key) {
                *id
            } else {
                let address = nodes.len() as u64 + 1;
                if let Some(node) = nodes.iter_mut().find(|n| n.address == parent) {
                    node.children.push(address);
                }
                nodes.push(DisplayNode {
                    address,
                    kind: "container",
                    name: parts[end - 1].into(),
                    parent,
                    children: vec![],
                    text: None,
                    visible: true,
                    alpha: Some(1.0),
                    screen_bounds: None,
                    unavailable: None,
                });
                ids.insert(key, address);
                address
            };
            parent = address;
        }
        let leaf = nodes.iter_mut().find(|n| n.address == parent).unwrap();
        leaf.text = text.map(str::to_owned);
        leaf.screen_bounds = *bounds;
    }
    DisplayTree {
        movie: Movie {
            path: POPUP.into(),
            record_address: 1,
            path_address: 2,
            width: 2560,
            height: 1440,
            scale_x: 0.625,
            scale_y: 0.625,
        },
        nodes,
        unrecognized_objects: 0,
        consistent: true,
        total_ms: 0,
    }
}

fn popup() -> DisplayTree {
    tree(&[
        (
            "Popup.Title",
            Some("<p><font color=\"#F5E3AD\">AKJAGARA PRIME BARREL</font></p>"),
            None,
        ),
        ("Popup.TitleBg.", None, Some([934.4, 264.0, 1494.4, 516.8])),
        (
            "Popup.TagContainer.Tag1.Label",
            Some("<p>\u{e0b6} 45</p>"),
            Some([958.4, 344.0, 1078.4, 386.88]),
        ),
    ])
}

#[test]
fn kiosk_requires_own_visible_title_not_inventory_category() {
    let mut gate = tree(&[(
        "TitleBar.Title.Label",
        Some("<p>INVENTORY/<font>DUCAT KIOSK </font></p>"),
        None,
    )]);
    assert!(kiosk_open(&gate).unwrap());
    gate.nodes.last_mut().unwrap().visible = false;
    assert!(!kiosk_open(&gate).unwrap());
    gate.nodes.last_mut().unwrap().visible = true;
    gate.nodes.last_mut().unwrap().text = Some("INVENTORY/SELL ITEMS".into());
    assert!(!kiosk_open(&gate).unwrap());
    gate.consistent = false;
    assert!(kiosk_open(&gate).is_err());
    assert!(kiosk_open(&tree(&[("CurrentCategory", Some("PRIME PARTS"), None)])).is_err());
}

#[test]
fn reads_visible_tooltip_and_ignores_hidden_retained_title() {
    let mut tree = popup();
    let result = hover(&tree).unwrap().unwrap();
    assert_eq!(result.title, "AKJAGARA PRIME BARREL");
    assert_eq!(result.ducats, 45);
    assert_eq!(result.scale, 1.6);
    tree.nodes
        .iter_mut()
        .find(|n| n.name == "Title")
        .unwrap()
        .visible = false;
    assert!(hover(&tree).unwrap().is_none());
    tree.consistent = false;
    assert!(hover(&tree).is_err());
}

#[test]
fn rejects_unavailable_outside_or_ambiguous_price_geometry() {
    let mut tree = popup();
    tree.nodes.last_mut().unwrap().screen_bounds = Some([0.0, 0.0, 20.0, 20.0]);
    assert!(hover(&tree).is_err());
    tree = popup();
    tree.nodes.last_mut().unwrap().unavailable = Some("changed definition".into());
    assert!(hover(&tree).is_err());
    tree = popup();
    tree.nodes.last_mut().unwrap().text = Some("\u{e0b6} not a price".into());
    assert!(hover(&tree).is_err());
    tree = popup();
    tree.nodes.last_mut().unwrap().text = Some("25 OWNED".into());
    assert!(hover(&tree).unwrap().is_none());
}

#[test]
fn cached_identity_tolerates_text_and_motion_but_not_replaced_children() {
    let mut tree = popup();
    let selection = Selection::new(&tree, true).unwrap();
    assert!(selection.matches(&tree));
    tree.nodes.last_mut().unwrap().text = Some("\u{e0b6} 100".into());
    tree.nodes.last_mut().unwrap().screen_bounds = Some([1000.0, 400.0, 1200.0, 450.0]);
    assert!(selection.matches(&tree));
    tree.nodes
        .iter_mut()
        .find(|n| n.name == "TagContainer")
        .unwrap()
        .children
        .push(999);
    assert!(!selection.matches(&tree));
}

#[test]
fn literal_item_text_keeps_punctuation_and_rejects_broken_tags() {
    assert_eq!(
        plain_text("<p>ACK &amp; BRUNT</p>"),
        Some("ACK & BRUNT".into())
    );
    assert_eq!(plain_text("<font color=\"white\""), None);
    let mut movies = vec![popup().movie];
    assert!(unique_movie(&movies, POPUP).unwrap().is_some());
    movies.push(movies[0].clone());
    assert!(unique_movie(&movies, POPUP).is_err());
}
