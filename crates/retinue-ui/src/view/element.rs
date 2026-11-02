use std::{any::Any, rc::Rc};

use super::primitive::ErasedPrimitive;
use super::{Rectangle, View, ViewContext, environment::Provider};
use crate::{Key, scene::Algorithm};

#[derive(Clone)]
pub struct Element {
    pub(crate) key: Option<Key>,
    pub(crate) layout_values: crate::layout::values::LayoutValues,
    pub(crate) input: Option<crate::input::InputOptions>,
    pub(crate) animation: Option<crate::motion::AnimationScope>,
    pub(crate) attributes: super::properties::Attributes,
    pub(crate) kind: Rc<Kind>,
}

impl Element {
    pub fn fragment(children: impl super::IntoChildren) -> Self {
        Self::new(Kind::Fragment(children.into_children()))
    }
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            key: None,
            layout_values: Default::default(),
            input: None,
            animation: None,
            attributes: Default::default(),
            kind: Rc::new(kind),
        }
    }

    pub(crate) fn same_type(&self, other: &Self) -> bool {
        match (&*self.kind, &*other.kind) {
            (Kind::View(a), Kind::View(b)) => a.as_any().type_id() == b.as_any().type_id(),
            (Kind::Primitive(a), Kind::Primitive(b)) => a.value().type_id() == b.value().type_id(),
            (Kind::Layout(a), Kind::Layout(b)) => a.kind == b.kind,
            (Kind::Environment(a, _), Kind::Environment(b, _)) => {
                a.as_any().type_id() == b.as_any().type_id()
            }
            (Kind::Rectangle(_), Kind::Rectangle(_)) | (Kind::Empty, Kind::Empty) => true,
            (Kind::Fragment(_), Kind::Fragment(_)) => true,
            _ => false,
        }
    }
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        if self.key != other.key
            || self.layout_values != other.layout_values
            || self.input != other.input
            || self.animation != other.animation
            || self.attributes != other.attributes
        {
            return false;
        }
        if Rc::ptr_eq(&self.kind, &other.kind) {
            return true;
        }
        match (&*self.kind, &*other.kind) {
            (Kind::View(a), Kind::View(b)) => a.equals(&**b),
            (Kind::Primitive(a), Kind::Primitive(b)) => a.equals(&**b),
            (Kind::Layout(a), Kind::Layout(b)) => a == b,
            (Kind::Environment(a, x), Kind::Environment(b, y)) => a.equals(&**b) && x == y,
            (Kind::Rectangle(a), Kind::Rectangle(b)) => a == b,
            (Kind::Empty, Kind::Empty) => true,
            (Kind::Fragment(a), Kind::Fragment(b)) => a == b,
            _ => false,
        }
    }
}

pub trait IntoElement {
    fn into_element(self) -> Element;
}

impl IntoElement for Element {
    fn into_element(self) -> Element {
        self
    }
}

impl<V: View> IntoElement for V {
    fn into_element(self) -> Element {
        Element::new(Kind::View(Rc::new(self)))
    }
}

pub(crate) trait ErasedView {
    fn name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ErasedView) -> bool;
    fn body(&self, context: &mut ViewContext<'_>) -> Element;
}

impl<V: View> ErasedView for V {
    fn name(&self) -> &'static str {
        std::any::type_name::<V>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn ErasedView) -> bool {
        other.as_any().downcast_ref::<V>() == Some(self)
    }
    fn body(&self, context: &mut ViewContext<'_>) -> Element {
        View::body(self, context).into_element()
    }
}

pub(crate) enum Kind {
    View(Rc<dyn ErasedView>),
    Primitive(Rc<dyn ErasedPrimitive>),
    Layout(LayoutDescription),
    Environment(Rc<dyn Provider>, Element),
    Rectangle(Rectangle),
    Empty,
    Fragment(Vec<Element>),
}

impl Kind {
    pub fn transparent(&self) -> bool {
        matches!(self, Self::View(_) | Self::Environment(..))
    }

    pub fn same_output(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Layout(a), Self::Layout(b)) => {
                a.kind == b.kind
                    && a.style == b.style
                    && a.algorithm == b.algorithm
                    && a.properties == b.properties
            }
            (Self::Rectangle(a), Self::Rectangle(b)) => a == b,
            (Self::Primitive(a), Self::Primitive(b)) => a.equals(&**b),
            (Self::Empty, Self::Empty) => true,
            (Self::Fragment(_), Self::Fragment(_)) => true,
            (Self::View(_), Self::View(_)) | (Self::Environment(..), Self::Environment(..)) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutKind {
    HStack,
    VStack,
    ZStack,
    Frame,
    Padding,
    Decoration,
    Properties,
    Input,
    ScrollViewport,
    Custom,
}

#[derive(PartialEq)]
pub(crate) struct LayoutDescription {
    pub kind: LayoutKind,
    pub style: taffy::Style,
    pub algorithm: Algorithm,
    pub children: Vec<Element>,
    pub properties: super::properties::Properties,
}

impl LayoutKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::HStack => "HStack",
            Self::VStack => "VStack",
            Self::ZStack => "ZStack",
            Self::Frame => "Frame",
            Self::Padding => "Padding",
            Self::Decoration => "Decoration",
            Self::Properties => "Presentation",
            Self::Input => "Input",
            Self::ScrollViewport => "ScrollViewport",
            Self::Custom => "Layout",
        }
    }
}
