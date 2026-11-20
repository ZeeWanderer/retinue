# Build A View

A [View](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/authoring/trait.View/)
describes content from its inputs. Its `body` returns other views,
structural layouts or drawing primitives. Start with the SDK prelude:

```rust
use retinue_ui::prelude::*;

#[derive(PartialEq)]
struct RankSummary {
    name: String,
    rank: State<u32>,
}

#[view]
impl View for RankSummary {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            VStack(8.0) {
                Text(self.name.clone()).font_size(20.0)
                HStack(6.0) {
                    Text("Rank")
                    Text(self.rank.get().to_string())
                }
            }.alignment(HorizontalAlignment::Leading)
                .padding(12.0)
                .background(Rectangle(Color::rgb(0x252b30)).corner_radius(6.0))
        }
    }
}

let rank = State::new(0);
let view = RankSummary { name: "Karak".into(), rank: rank.clone() };
let mut graph = retinue_ui::ir::InstanceGraph::with_root_view(view, Environment::default()).unwrap();
graph.update_inline((320, 100), 1.0).unwrap();
rank.set(1);
graph.update_inline((320, 100), 1.0).unwrap();
```

`ui!` abbreviates constructors and child lists; it does not introduce a separate
runtime. `VStack::new(8.0, children)` is equally valid. Ordinary expressions and
struct literals work inside a child block. A named view is useful for reusable
content and dependency boundaries, not required for every rectangle.

## What Updates

Reading `rank` while evaluating `body` establishes a dependency. Changing it marks
that consumer dirty and schedules host work. Comparable child descriptions let
unchanged bodies remain retained. Model handles compare identity, so changing a
field does not require copying the whole model.

`PartialEq` must cover the view inputs that affect its body. Do not fetch data,
start tasks or mutate models in `body`; event handlers and owning services perform
those operations. Workers publish results through the host's event path.

## What Owns The Window

The example describes content; it does not open a native window. A
[InstanceGraph](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/ir/struct.InstanceGraph/)
owns mounted identities and updates; the host supplies size, scale, clock, input
and redraw scheduling. Use the desktop or overlay host rather than adding a
window lifetime to the view.

Run `make view-demo`. Its [root view](../../crates/retinue-ui/examples/views/model/root.rs)
shows a working hierarchy, and its [host](../../crates/retinue-ui/examples/views/main.rs)
shows the platform boundary. The example above is also compiled by the SDK's
documentation tests.

Continue with [views and identity](views.md), [state](state.md), or
[window and overlay hosts](hosts.md).
