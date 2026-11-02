use ui_alias::{drawing::TextStyle, prelude::*, runtime::ViewTree};

#[derive(PartialEq)]
struct Header {
    label: &'static str,
}
impl View for Header {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Text::new(self.label)
    }
}

#[test]
fn child_blocks_and_renamed_runtime_compile_and_match_constructor_output() {
    let values = ["one", "two", "three"];
    let declared = ui! {
        VStack(6.0) {
            Header { label: "Title" }
            if true { Text("Shown") } else { Text("Hidden") }
            for (index, name) in values.into_iter().enumerate() {
                Text(name).id(index as u64)
            }
            match values.len() { 0 => { Text("None") }, _ => {} }
        }.padding(4.0).environment::<Typography>(TextStyle::default())
    };
    let explicit = VStack::new(
        6.0,
        (
            Header { label: "Title" },
            Text::new("Shown"),
            Text::new("one").id(0),
            Text::new("two").id(1),
            Text::new("three").id(2),
        ),
    )
    .padding(4.0)
    .environment::<Typography>(TextStyle::default());
    let mut a = ViewTree::new(declared, || {});
    let mut b = ViewTree::new(explicit, || {});
    a.update((400, 300), 1.0).unwrap();
    b.update((400, 300), 1.0).unwrap();
    assert_eq!(
        a.frame()
            .instances()
            .iter()
            .map(|x| x.visible_bounds())
            .collect::<Vec<_>>(),
        b.frame()
            .instances()
            .iter()
            .map(|x| x.visible_bounds())
            .collect::<Vec<_>>()
    );
    for (a, b) in a.frame().instances().iter().zip(b.frame().instances()) {
        assert!(a.program.commands() == b.program.commands());
    }
}

#[derive(PartialEq)]
struct Conditional {
    shown: State<bool>,
    left: State<u32>,
    right: State<u32>,
}
impl View for Conditional {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            VStack(4.0) {
                if self.shown.get() {
                    Text(self.left.get().to_string())
                } else {
                    Text(self.right.get().to_string())
                }
            }
        }
    }
}

#[test]
fn conditional_reads_use_the_existing_dynamic_observation() {
    let shown = State::new(true);
    let left = State::new(1);
    let right = State::new(2);
    let mut tree = ViewTree::new(
        Conditional {
            shown: shown.clone(),
            left: left.clone(),
            right: right.clone(),
        },
        || {},
    );
    tree.update((200, 120), 1.0).unwrap();
    right.set(3);
    assert!(!tree.needs_update());
    shown.set(false);
    tree.update((200, 120), 1.0).unwrap();
    left.set(4);
    assert!(!tree.needs_update());
    right.set(5);
    assert!(tree.needs_update());
}

#[test]
fn single_primitive_and_rust_expression_escape_are_supported() {
    let _ = ui! { Text("Title").font_size(20.0) };
    let _ = ui! { { let label = "Regular Rust"; Header { label } } };
    let _ = ui! { HStack::new(3.0) { Text::new("Normal constructors") } };
    let _ = ui! {
        Text("A").padding(4.0).background { Rectangle(Color::rgb(0x223344)) }
            .overlay_aligned(ui_alias::layout::anchors::AnchorPoint::TOP_TRAILING) {
                Text("Badge")
            }
    };
}
