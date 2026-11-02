use super::ViewContext;

/// A view's retained handles and inherited values, resolved during body evaluation.
pub trait ViewProperties: Sized + 'static {
    fn resolve(context: &mut ViewContext<'_>) -> Self;
}
