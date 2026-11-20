use smithay_client_toolkit::seat::pointer::CursorIcon;

#[derive(Default)]
pub struct PublishedCursor(Option<CursorIcon>);

impl PublishedCursor {
    pub fn invalidate(&mut self) {
        self.0 = None;
    }

    pub fn publish<E>(&mut self, icon: CursorIcon, send: impl FnOnce(CursorIcon) -> Result<(), E>) {
        if self.0 != Some(icon) && send(icon).is_ok() {
            self.0 = Some(icon);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_on_change_reentry_and_after_failure() {
        use CursorIcon::{Default, Text};
        let mut cursor = PublishedCursor::default();
        let mut requests = Vec::new();
        for (icon, reset, succeeds) in [
            (Default, true, true),
            (Default, false, true),
            (Text, false, true),
            (Text, true, false),
            (Text, false, true),
            (Text, false, true),
        ] {
            if reset {
                cursor.invalidate();
            }
            cursor.publish(icon, |icon| {
                requests.push(icon);
                succeeds.then_some(()).ok_or(())
            });
        }
        assert_eq!(requests, [Default, Text, Text, Text]);
    }
}
