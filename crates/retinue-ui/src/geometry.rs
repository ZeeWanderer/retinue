#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self
                .x
                .saturating_add(self.width)
                .max(other.x.saturating_add(other.width))
                - x,
            height: self
                .y
                .saturating_add(self.height)
                .max(other.y.saturating_add(other.height))
                - y,
        }
    }

    pub fn expanded(self, margin: u32) -> Self {
        if self.is_empty() {
            return self;
        }
        let x = self.x.saturating_sub(margin);
        let y = self.y.saturating_sub(margin);
        Self {
            x,
            y,
            width: self.x.saturating_add(self.width).saturating_add(margin) - x,
            height: self.y.saturating_add(self.height).saturating_add(margin) - y,
        }
    }

    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && y >= f64::from(self.y)
            && x < f64::from(self.x + self.width)
            && y < f64::from(self.y + self.height)
    }

    pub fn scaled(self, scale: u32) -> Self {
        Self {
            x: self.x * scale,
            y: self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
        }
    }

    pub fn inset(self, inset: u32) -> Self {
        let horizontal = inset.saturating_mul(2).min(self.width);
        let vertical = inset.saturating_mul(2).min(self.height);
        Self {
            x: self.x + horizontal / 2,
            y: self.y + vertical / 2,
            width: self.width - horizontal,
            height: self.height - vertical,
        }
    }

    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        Self {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}
