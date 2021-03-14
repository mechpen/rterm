pub enum CharsetIndex {
    G0 = 0,
    G1 = 1,
    G2 = 2,
    G3 = 3,
}

#[derive(Clone, Copy)]
pub enum Charset {
    Graphic0,
    Ascii,
}

impl Charset {
    pub fn map(&self, c: char) -> char {
        match self {
            Charset::Ascii => c,
            Charset::Graphic0 => match c {
                '`' => '◆',
                'a' => '▒',
                'b' => '\t',
                'c' => '\u{000c}',
                'd' => '\r',
                'e' => '\n',
                'f' => '°',
                'g' => '±',
                'h' => '\u{2424}',
                'i' => '\u{000b}',
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}

pub struct CharsetTable {
    charsets: [Charset; 4],
    current: usize,
}

impl CharsetTable {
    pub fn new() -> Self {
        Self {
            charsets: [Charset::Ascii; 4],
            current: 0,
        }
    }

    pub fn setup(&mut self, index: CharsetIndex, charset: Charset) {
        let index = index as usize;
        self.charsets[index] = charset;
    }

    pub fn set_current(&mut self, index: CharsetIndex) {
        self.current = index as usize;
    }

    pub fn map(&self, c: char) -> char {
        self.charsets[self.current].map(c)
    }
}
