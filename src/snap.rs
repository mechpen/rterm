use std::time::{
    Instant,
    Duration,
};

const WORD_DELIMITERS: &str = " ()[]{}<>`~!@#$%^&*-=+\\|;:'\",.?/";

pub fn is_delim(c: char) -> bool {
    WORD_DELIMITERS.contains(c)
}

const DOUBLE_CLICK_TIMEOUT: Duration = Duration::from_millis(300);
const TRIPLE_CLICK_TIMEOUT: Duration = Duration::from_millis(600);

pub enum SnapMode {
    None,
    Word,
    Line,
}

pub struct Snap {
    click1: Instant,
    click2: Instant,
}

impl Snap {
    pub fn new() -> Self {
        let now = Instant::now();
        Snap {
            click1: now,
            click2: now,
        }
    }

    pub fn click(&mut self) -> SnapMode {
        let now = Instant::now();
        let mut mode = SnapMode::None;

        if now.duration_since(self.click2) < TRIPLE_CLICK_TIMEOUT {
            mode = SnapMode::Line;
        } else if now.duration_since(self.click1) < DOUBLE_CLICK_TIMEOUT {
            mode = SnapMode::Word;
        }

        self.click2 = self.click1;
        self.click1 = now;
        mode
    }
}
