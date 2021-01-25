use std::time;

const WORD_DELIMITERS: &[u8] = b" ()[]{}<>`~!@#$%^&*-=+\\|;:'\",.?/";

pub fn is_delim(u: u32) -> bool {
    WORD_DELIMITERS.contains(&(u as u8))
}

// FIXME: lazy static
const DOUBLE_CLICK_TIMEOUT_MS: u128 = 300;
const TRIPLE_CLICK_TIMEOUT_MS: u128 = 600;

pub enum SnapMode {
    None,
    Word,
    Line,
}

pub struct SnapClick {
    click1: time::Instant,
    click2: time::Instant,
}

impl SnapClick {
    pub fn new() -> Self {
        let now = time::Instant::now();
        SnapClick {
            click1: now,
            click2: now,
        }
    }

    pub fn click(&mut self) -> SnapMode {
        let now = time::Instant::now();
        let mut snap = SnapMode::None;

        if now.duration_since(self.click2).as_millis() < TRIPLE_CLICK_TIMEOUT_MS {
            snap = SnapMode::Line;
        } else if now.duration_since(self.click1).as_millis() < DOUBLE_CLICK_TIMEOUT_MS {
            snap = SnapMode::Word;
        }

        self.click2 = self.click1;
        self.click1 = now;
        snap
    }
}
