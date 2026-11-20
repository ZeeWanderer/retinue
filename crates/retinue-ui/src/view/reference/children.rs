use super::{Element, IntoElement};

pub trait IntoChildren {
    fn into_children(self) -> Vec<Element>;
}

impl IntoChildren for () {
    fn into_children(self) -> Vec<Element> {
        Vec::new()
    }
}

impl<T: IntoElement, const N: usize> IntoChildren for [T; N] {
    fn into_children(self) -> Vec<Element> {
        self.into_iter().map(IntoElement::into_element).collect()
    }
}

impl<T: IntoElement> IntoChildren for Vec<T> {
    fn into_children(self) -> Vec<Element> {
        self.into_iter().map(IntoElement::into_element).collect()
    }
}

macro_rules! tuple_children {
    ($($name:ident),+) => {
        impl<$($name: IntoElement),+> IntoChildren for ($($name,)+) {
            #[allow(non_snake_case)]
            fn into_children(self) -> Vec<Element> {
                let ($($name,)+) = self;
                vec![$($name.into_element()),+]
            }
        }
    };
}

tuple_children!(A);
tuple_children!(A, B);
tuple_children!(A, B, C);
tuple_children!(A, B, C, D);
tuple_children!(A, B, C, D, E);
tuple_children!(A, B, C, D, E, F);
tuple_children!(A, B, C, D, E, F, G);
tuple_children!(A, B, C, D, E, F, G, H);
