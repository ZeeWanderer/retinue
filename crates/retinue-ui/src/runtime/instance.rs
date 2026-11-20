use crate::{
    Key,
    layout::values::LayoutValues,
    view::{self, Element, ErasedView, LayoutInput, Provider, Rectangle},
};
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct Input {
    pub key: Option<Key>,
    pub layout_values: LayoutValues,
    pub input: Option<crate::input::InputOptions>,
    pub animation: Option<crate::motion::AnimationScope>,
    pub attributes: view::Attributes,
    pub kind: Kind,
}

#[derive(Clone)]
pub(crate) enum Kind {
    View(Rc<dyn ErasedView>),
    Primitive(Rc<dyn view::primitive::ErasedPrimitive>),
    Layout(Rc<LayoutInput>),
    Environment(Rc<dyn Provider>),
    Rectangle(Rectangle),
    Empty,
    Fragment,
}

impl Input {
    pub fn matches(&self, other: &Element) -> bool {
        self.key == other.key
            && self.layout_values == other.layout_values
            && self.input == other.input
            && self.animation == other.animation
            && self.attributes == other.attributes
            && match (&self.kind, &*other.kind) {
                (Kind::View(a), view::Kind::View(b)) => Rc::ptr_eq(a, b) || a.equals(&**b),
                (Kind::Primitive(a), view::Kind::Primitive(b)) => {
                    Rc::ptr_eq(a, b) || a.equals(&**b)
                }
                (Kind::Environment(a), view::Kind::Environment(b, _)) => {
                    Rc::ptr_eq(a, b) || a.equals(&**b)
                }
                (Kind::Layout(a), view::Kind::Layout(b, _)) => Rc::ptr_eq(a, b) || a == b,
                (Kind::Rectangle(a), view::Kind::Rectangle(b)) => a == b,
                (Kind::Empty, view::Kind::Empty) | (Kind::Fragment, view::Kind::Fragment(_)) => {
                    true
                }
                _ => false,
            }
    }

    pub fn take(element: Element) -> (Self, Option<Vec<Element>>) {
        let Element {
            key,
            layout_values,
            input,
            animation,
            attributes,
            kind,
        } = element;
        let kind = Rc::try_unwrap(kind).unwrap_or_else(|kind| (*kind).clone());
        let (kind, children) = match kind {
            view::Kind::View(view) => (Kind::View(view), None),
            view::Kind::Primitive(primitive) => (Kind::Primitive(primitive), None),
            view::Kind::Layout(layout, children) => (Kind::Layout(layout), Some(children)),
            view::Kind::Environment(provider, child) => {
                (Kind::Environment(provider), Some(vec![child]))
            }
            view::Kind::Rectangle(rectangle) => (Kind::Rectangle(rectangle), None),
            view::Kind::Empty => (Kind::Empty, None),
            view::Kind::Fragment(children) => (Kind::Fragment, Some(children)),
        };
        (
            Self {
                key,
                layout_values,
                input,
                animation,
                attributes,
                kind,
            },
            children,
        )
    }

    pub fn compatible(&self, other: &Element) -> bool {
        self.key == other.key
            && match (&self.kind, &*other.kind) {
                (Kind::View(a), view::Kind::View(b)) => {
                    a.as_any().type_id() == b.as_any().type_id()
                }
                (Kind::Primitive(a), view::Kind::Primitive(b)) => {
                    a.value().type_id() == b.value().type_id()
                }
                (Kind::Layout(a), view::Kind::Layout(b, _)) => a.kind == b.kind,
                (Kind::Environment(a), view::Kind::Environment(b, _)) => {
                    a.as_any().type_id() == b.as_any().type_id()
                }
                (Kind::Rectangle(_), view::Kind::Rectangle(_))
                | (Kind::Empty, view::Kind::Empty)
                | (Kind::Fragment, view::Kind::Fragment(_)) => true,
                _ => false,
            }
    }
}

impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.layout_values == other.layout_values
            && self.input == other.input
            && self.animation == other.animation
            && self.attributes == other.attributes
            && match (&self.kind, &other.kind) {
                (Kind::View(a), Kind::View(b)) => Rc::ptr_eq(a, b) || a.equals(&**b),
                (Kind::Primitive(a), Kind::Primitive(b)) => Rc::ptr_eq(a, b) || a.equals(&**b),
                (Kind::Environment(a), Kind::Environment(b)) => Rc::ptr_eq(a, b) || a.equals(&**b),
                (Kind::Layout(a), Kind::Layout(b)) => Rc::ptr_eq(a, b) || a == b,
                (Kind::Rectangle(a), Kind::Rectangle(b)) => a == b,
                (Kind::Empty, Kind::Empty) | (Kind::Fragment, Kind::Fragment) => true,
                _ => false,
            }
    }
}

impl Kind {
    pub fn transparent(&self) -> bool {
        matches!(self, Self::View(_) | Self::Environment(_))
    }

    pub fn same_output(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Layout(a), Self::Layout(b)) => Rc::ptr_eq(a, b) || a == b,
            (Self::Rectangle(a), Self::Rectangle(b)) => a == b,
            (Self::Primitive(a), Self::Primitive(b)) => Rc::ptr_eq(a, b) || a.equals(&**b),
            (Self::Empty, Self::Empty)
            | (Self::Fragment, Self::Fragment)
            | (Self::View(_), Self::View(_))
            | (Self::Environment(_), Self::Environment(_)) => true,
            _ => false,
        }
    }
}
