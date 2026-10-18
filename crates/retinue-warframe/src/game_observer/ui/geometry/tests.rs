use super::*;

fn movie() -> Movie {
    Movie {
        path: "test.swf".into(),
        record_address: 0x10c0,
        path_address: 0,
        width: 2560,
        height: 1440,
        scale_x: 0.625,
        scale_y: 0.625,
    }
}

fn node(parent: u64, children: Vec<u64>) -> Node {
    let mut matrix = [0.0; 16];
    for index in [0, 5, 10, 15] {
        matrix[index] = 1.0;
    }
    Node {
        header: Vec::new(),
        matrix,
        local_rect: None,
        alpha: [1.0, 0.0],
        visible: true,
        parent_alive: true,
        text_pointer: 0,
        result: DisplayNode {
            address: 0,
            kind: "container",
            name: String::new(),
            parent,
            children,
            text: None,
            visible: false,
            alpha: None,
            screen_bounds: None,
            unavailable: None,
        },
    }
}

#[test]
fn projects_twips_through_parent_chain_and_movie_scale() {
    let mut root = node(0, vec![2]);
    root.matrix[12] = 12000.0;
    root.matrix[13] = 6000.0;
    let mut leaf = node(1, vec![]);
    leaf.local_rect = Some([0.0, 640.0, 0.0, 640.0]);
    leaf.matrix[0] = 2.0;
    let nodes = BTreeMap::from([(1, root), (2, leaf)]);
    let (visible, alpha, bounds) = project(2, &nodes, &movie()).unwrap();
    assert!(visible);
    assert_eq!(alpha, 1.0);
    assert_eq!(bounds, Some([960.0, 480.0, 1062.4, 531.2]));
}

#[test]
fn hidden_retained_text_is_not_visible() {
    let mut root = node(0, vec![2]);
    root.alpha = [0.0, 0.0];
    let mut leaf = node(1, vec![]);
    leaf.result.text = Some("ORTHOS PRIME BLUEPRINT".into());
    let mut nodes = BTreeMap::from([(1, root), (2, leaf)]);
    assert!(!project(2, &nodes, &movie()).unwrap().0);
    nodes.get_mut(&1).unwrap().alpha = [1.0, 0.0];
    nodes.get_mut(&1).unwrap().visible = false;
    assert!(!project(2, &nodes, &movie()).unwrap().0);
}

#[test]
fn rejects_stale_cyclic_or_inconsistent_parentage() {
    let mut nodes = BTreeMap::from([(1, node(0, vec![2])), (2, node(1, vec![]))]);
    nodes.get_mut(&2).unwrap().parent_alive = false;
    assert!(
        project(2, &nodes, &movie())
            .unwrap_err()
            .contains("expired")
    );
    nodes.get_mut(&2).unwrap().parent_alive = true;
    nodes.get_mut(&1).unwrap().result.parent = 2;
    nodes.get_mut(&2).unwrap().result.children.push(1);
    assert!(project(2, &nodes, &movie()).unwrap_err().contains("cycle"));
    nodes.get_mut(&1).unwrap().result.parent = 0;
    nodes.get_mut(&1).unwrap().result.children.clear();
    assert!(
        project(2, &nodes, &movie())
            .unwrap_err()
            .contains("disagree")
    );
}

#[test]
fn transforms_four_dimensions_before_projecting() {
    let mut root = node(0, vec![2]);
    root.matrix[8] = 1.0;
    let mut leaf = node(1, vec![]);
    leaf.matrix[14] = 20.0;
    leaf.local_rect = Some([0.0, 20.0, 0.0, 20.0]);
    let mut nodes = BTreeMap::from([(1, root), (2, leaf)]);
    assert_eq!(
        project(2, &nodes, &movie()).unwrap().2,
        Some([1.6, 0.0, 3.2, 1.6])
    );
    nodes.get_mut(&2).unwrap().matrix[15] = 0.0;
    assert!(project(2, &nodes, &movie()).is_err());
}

#[test]
fn rejects_nonfinite_input() {
    assert!(floats::<1>(&f32::NAN.to_le_bytes(), 0).is_err());
    let mut root = node(0, vec![]);
    root.alpha[0] = f32::INFINITY;
    assert!(project(1, &BTreeMap::from([(1, root)]), &movie()).is_err());
}

#[test]
fn reads_typed_tree_and_rejects_changed_types_or_vectors() {
    let display = crate::game_observer::adapter::test_scaleform();
    let image = 0x1_4000_0000;
    let layout = layout::Layout {
        matrix: 0x78,
        parent_control: 0xb8,
        parent: 0xc0,
        name: 0xc8,
        alpha: 0xf0,
        flags: 0x127,
        leaf_definitions: BTreeMap::from([(display.text_vtable_rva, 0x130)]),
        definition_rects: BTreeMap::from([(0x9000, 0x50)]),
    };
    let mut bytes = vec![0; 0x10000];
    fn u64_value(bytes: &mut [u8], at: usize, value: u64) {
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn vector(bytes: &mut [u8], at: usize, pointer: u64) {
        u64_value(bytes, at, pointer);
        bytes[at + 8..at + 12].copy_from_slice(&8_u32.to_le_bytes());
        bytes[at + 12..at + 16].copy_from_slice(&8_u32.to_le_bytes());
    }
    u64_value(
        &mut bytes,
        0x1000,
        image + display.flash_instance_vtable_rva,
    );
    u64_value(&mut bytes, 0x1088, 0x2000);
    u64_value(&mut bytes, 0x2000, image + display.root_vtable_rva);
    u64_value(
        &mut bytes,
        0x2010,
        image + display.root_secondary_vtable_rva,
    );
    u64_value(&mut bytes, 0x20a0, 0x1000);
    vector(&mut bytes, 0x2028, 0x3000);
    u64_value(&mut bytes, 0x3000, 0x4000);
    for (at, primary, secondary) in [
        (
            0x4000,
            display.container_vtable_rva,
            display.container_secondary_vtable_rva,
        ),
        (
            0x6000,
            display.text_vtable_rva,
            display.text_secondary_vtable_rva,
        ),
    ] {
        u64_value(&mut bytes, at, image + primary);
        u64_value(&mut bytes, at + 0x10, image + secondary);
        for index in [0, 5, 10, 15] {
            bytes[at + 0x78 + index * 4..at + 0x7c + index * 4]
                .copy_from_slice(&1_f32.to_le_bytes());
        }
        bytes[at + 0xf0..at + 0xf4].copy_from_slice(&1_f32.to_le_bytes());
        bytes[at + 0x127] = 1;
    }
    bytes[0x40c8..0x40cd].copy_from_slice(b"Popup");
    vector(&mut bytes, 0x4130, 0x5000);
    u64_value(&mut bytes, 0x5000, 0x6000);
    u64_value(&mut bytes, 0x60b8, 0x7000);
    bytes[0x7008] = 1;
    u64_value(&mut bytes, 0x60c0, 0x4000);
    u64_value(&mut bytes, 0x60c8, 0x7100);
    bytes[0x60d7] = 0xff;
    bytes[0x7100..0x7113].copy_from_slice(b"LongDisplayInstance");
    u64_value(&mut bytes, 0x6130, 0x8000);
    u64_value(&mut bytes, 0x8000, image + 0x9000);
    for (i, value) in [0_f32, 640.0, 0.0, 640.0].iter().enumerate() {
        bytes[0x8050 + i * 4..0x8054 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    u64_value(&mut bytes, 0x6190, 0x9000);
    bytes[0x9000..0x9004].copy_from_slice(b"Test");
    let source = |bytes: &[u8]| {
        memory::from_test_bytes(
            bytes,
            vec![
                memory::Region {
                    start: 0x1000,
                    end: bytes.len() as u64,
                    permissions: "rw-p".into(),
                    path: String::new(),
                },
                memory::Region {
                    start: image,
                    end: image + 0x10000,
                    permissions: "r--p".into(),
                    path: "/game/Warframe.x64.exe".into(),
                },
            ],
        )
    };
    let report = read_tree(&source(&bytes), display, &layout, &movie(), 10).unwrap();
    assert!(report.consistent);
    assert_eq!(report.nodes.len(), 2);
    let text = &report.nodes[1];
    assert_eq!(text.name, "LongDisplayInstance");
    assert_eq!(text.text.as_deref(), Some("Test"));
    assert!(text.visible);
    assert_eq!(text.screen_bounds, Some([0.0, 0.0, 51.2, 51.2]));
    let selected = read_selected(&source(&bytes), display, &layout, &movie(), &[0x6000]).unwrap();
    assert!(selected.consistent);
    assert_eq!(selected.nodes.len(), 2);
    assert_eq!(selected.nodes[1].screen_bounds, text.screen_bounds);
    u64_value(&mut bytes, 0x3000, 0x6000);
    assert!(read_selected(&source(&bytes), display, &layout, &movie(), &[0x6000]).is_err());
    u64_value(&mut bytes, 0x3000, 0x4000);
    bytes[0x6010] ^= 1;
    assert!(read_tree(&source(&bytes), display, &layout, &movie(), 10).is_err());
    bytes[0x6010] ^= 1;
    assert!(read_tree(&source(&bytes), display, &layout, &movie(), 1).is_err());
    bytes[0x40f0..0x40f4].fill(0);
    assert!(
        !read_tree(&source(&bytes), display, &layout, &movie(), 10)
            .unwrap()
            .nodes[1]
            .visible
    );
    bytes[0x4138] = 7;
    assert!(read_tree(&source(&bytes), display, &layout, &movie(), 10).is_err());
    bytes[0x4138] = 8;
    bytes[0x2010] ^= 1;
    assert!(read_tree(&source(&bytes), display, &layout, &movie(), 10).is_err());
}
