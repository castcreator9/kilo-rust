pub mod terminal;

use nix::sys::termios::Termios;

use crate::terminal::enable_raw_mode;
use std::io::{self, Read, Write};

// Util functions
fn ctrl_key(k: char) -> char {
    ((k as u8) & 0x1f) as char
}

fn write_stdout(buf: &[u8]) {
    io::stdout()
        .write(buf)
        .expect("Error while writing to stdout.");
    io::stdout().flush().expect("Error while flushing stdout");
}

enum EditorKey {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    DelKey,
    HomeKey,
    EndKey,
    PageUp,
    PageDown,
    Char(char),
}

// This is E in kilo
struct Editor {
    cx: usize,
    cy: usize,
    screen_rows: usize,
    screen_cols: usize,
    buffer: String,
    _orig_termios: Termios,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            cx: 0,
            cy: 0,
            screen_rows: 0,
            screen_cols: 0,
            buffer: String::new(),
            _orig_termios: enable_raw_mode(),
        }
    }

    pub fn get_window_size(&mut self) {
        // Move the cursor as far right and down
        write_stdout(b"\x1b[999C\x1b[999B");
        // Ask in what position is it the cursor
        write_stdout(b"\x1b[6n");

        let mut response = Vec::new();
        let mut buffer = [0u8; 1];

        while let Ok(_) = io::stdin().read(&mut buffer) {
            response.push(buffer[0]);

            if buffer[0] == b'R' {
                break;
            }
        }

        if response.len() < 2 || response[0] != b'\x1b' || response[1] != b'[' {
            return;
        }

        let response_str = String::from_utf8_lossy(&response[2..response.len() - 1]);
        let mut parts = response_str.split(';');

        self.screen_rows = parts.next().unwrap().parse::<usize>().unwrap();
        self.screen_cols = parts.next().unwrap().parse::<usize>().unwrap();
    }

    fn read_key() -> EditorKey {
        let mut buffer = [0u8; 4];

        // Read 1 char by time, if that char is an escape, read the next 3 chars
        while let Ok(n) = io::stdin().read(&mut buffer[..1]) {
            if n == 1 && buffer[0] == b'\x1b' {
                let _ = io::stdin().read(&mut buffer[1..4]);
                break;
            } else if n == 1 {
                break;
            }
        }

        // TODO: Change all EditorKey::Char('\x1b') to None
        if buffer[0] == b'\x1b' && buffer[1] == b'[' {
            if buffer[2].is_ascii_digit() {
                if buffer[3] == b'~' {
                    match buffer[2] as char {
                        '1' => return EditorKey::HomeKey,
                        '3' => return EditorKey::DelKey,
                        '4' => return EditorKey::EndKey,
                        '5' => return EditorKey::PageUp,
                        '6' => return EditorKey::PageDown,
                        '7' => return EditorKey::HomeKey,
                        '8' => return EditorKey::EndKey,
                        _ => return EditorKey::Char('\x1b'),
                    }
                } else {
                    return EditorKey::Char('\x1b');
                }
            } else {
                match buffer[2] as char {
                    'A' => return EditorKey::ArrowUp,
                    'B' => return EditorKey::ArrowDown,
                    'C' => return EditorKey::ArrowRight,
                    'D' => return EditorKey::ArrowLeft,
                    'H' => return EditorKey::HomeKey,
                    'F' => return EditorKey::EndKey,
                    _ => return EditorKey::Char('\x1b'),
                }
            }
        } else if buffer[1] == b'O' {
            match buffer[2] as char {
                'H' => return EditorKey::HomeKey,
                'F' => return EditorKey::EndKey,
                _ => return EditorKey::Char('\x1b'),
            }
        } else {
            return EditorKey::Char(buffer[0] as char);
        }
    }

    fn move_cursor(&mut self, key: EditorKey) {
        match key {
            EditorKey::ArrowLeft => {
                if self.cx != 0 {
                    self.cx -= 1;
                }
            }
            EditorKey::ArrowRight => {
                if self.cx != self.screen_cols - 1 {
                    self.cx += 1;
                }
            }
            EditorKey::ArrowUp => {
                if self.cy != 0 {
                    self.cy -= 1;
                }
            }
            EditorKey::ArrowDown => {
                if self.cy != self.screen_rows - 1 {
                    self.cy += 1;
                }
            }
            EditorKey::HomeKey => {
                self.cx = 0;
            }
            EditorKey::EndKey => {
                self.cx = self.screen_cols - 1;
            }
            EditorKey::PageUp => {
                let mut times = self.screen_rows;
                while times > 0 {
                    self.move_cursor(EditorKey::ArrowUp);
                    times -= 1;
                }
            }
            EditorKey::PageDown => {
                let mut times = self.screen_rows;
                while times > 0 {
                    self.move_cursor(EditorKey::ArrowDown);
                    times -= 1;
                }
            }
            EditorKey::Char(_) | EditorKey::DelKey => {
                return;
            }
        }
    }

    fn procress_key_press(&mut self) {
        let c = Editor::read_key();

        match c {
            EditorKey::ArrowUp
            | EditorKey::ArrowDown
            | EditorKey::ArrowLeft
            | EditorKey::ArrowRight
            | EditorKey::PageUp
            | EditorKey::PageDown
            | EditorKey::HomeKey
            | EditorKey::EndKey
            | EditorKey::DelKey => {
                self.move_cursor(c);
            }
            EditorKey::Char(c) => {
                if c == ctrl_key('q') {
                    write_stdout(b"\x1b[2J");
                    write_stdout(b"\x1b[H");
                    std::process::exit(0);
                }
            }
        }
    }

    fn editor_draw_rows(&mut self) {
        for y in 0..self.screen_rows {
            if y == self.screen_rows / 3 {
                let welcome = format!("Kilo editor -- version {}", env!("CARGO_PKG_VERSION"));
                let mut padding = (self.screen_cols - welcome.len()) / 2;
                if padding > 0 {
                    self.buffer.push_str("~");
                    padding -= 1;
                }
                while padding > 0 {
                    self.buffer.push_str(" ");
                    padding -= 1;
                }
                if welcome.len() > self.screen_cols {
                    self.buffer.push_str(&welcome[..self.screen_cols]);
                } else {
                    self.buffer.push_str(welcome.as_str());
                }
            } else {
                self.buffer.push_str("~");
            }

            self.buffer.push_str("\x1b[K");
            if y < self.screen_rows - 1 {
                self.buffer.push_str("\r\n");
            }
        }
    }

    fn refresh_screen(&mut self) {
        self.buffer.push_str("\x1b[?25l");
        self.buffer.push_str("\x1b[H");

        self.editor_draw_rows();

        let cursor_position = format!("\x1b[{};{}H", self.cy + 1, self.cx + 1);
        self.buffer.push_str(cursor_position.as_str());

        self.buffer.push_str("\x1b[?25h");
        write_stdout(self.buffer.as_bytes());
    }

    pub fn run(&mut self) {
        loop {
            self.refresh_screen();
            self.procress_key_press();
        }
    }
}

fn main() {
    let mut editor = Editor::new();
    editor.get_window_size();
    editor.run();
}
