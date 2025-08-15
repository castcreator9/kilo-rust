pub mod terminal;

use nix::sys::termios::Termios;

use crate::terminal::enable_raw_mode;
use std::{
    env,
    fs::File,
    io::{self, BufRead, Read, Write},
};

const KILO_TAB: usize = 8;

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
    rx: usize,
    row_off: usize,
    col_off: usize,
    screen_rows: usize,
    screen_cols: usize,
    buffer: String,
    num_rows: usize,
    row: Vec<String>,
    render: Vec<String>,
    filename: String,
    _orig_termios: Termios,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            cx: 0,
            cy: 0,
            rx: 0,
            row_off: 0,
            col_off: 0,
            screen_rows: 0,
            screen_cols: 0,
            buffer: String::new(),
            num_rows: 0,
            row: Vec::new(),
            render: Vec::new(),
            filename: String::new(),
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
        self.screen_rows -= 1; // Status bar
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
        let mut row = None;
        if self.cy < self.num_rows {
            row = Some(&self.row[self.cy]);
        }

        match key {
            EditorKey::ArrowLeft => {
                if self.cx != 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.row[self.cy].len();
                }
            }
            EditorKey::ArrowRight => {
                if let Some(row) = row {
                    if self.cx < row.len() {
                        self.cx += 1;
                    } else if self.cx == row.len() {
                        self.cy += 1;
                        self.cx = 0;
                    }
                }
            }
            EditorKey::ArrowUp => {
                if self.cy != 0 {
                    self.cy -= 1;
                }
            }
            EditorKey::ArrowDown => {
                if self.cy < self.num_rows {
                    self.cy += 1;
                }
            }
            EditorKey::HomeKey => {
                self.cx = 0;
            }
            EditorKey::EndKey => {
                if let Some(row) = row {
                    self.cx = row.len();
                }
            }
            EditorKey::PageUp => {
                self.cy = self.row_off;

                let mut times = self.screen_rows;
                while times > 0 {
                    self.move_cursor(EditorKey::ArrowUp);
                    times -= 1;
                }
                return;
            }
            EditorKey::PageDown => {
                self.cy = self.row_off + self.screen_rows - 1;
                if self.cy > self.num_rows {
                    self.cy = self.num_rows;
                }

                let mut times = self.screen_rows;
                while times > 0 {
                    self.move_cursor(EditorKey::ArrowDown);
                    times -= 1;
                }
                return;
            }
            EditorKey::Char(_) | EditorKey::DelKey => {
                return;
            }
        }

        if self.cy < self.num_rows {
            row = Some(&self.row[self.cy]);
        } else {
            row = None;
        }
        if let Some(row) = row {
            if self.cx > row.len() {
                self.cx = row.len();
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

    fn row_cx_to_rx(&self, row: &String) -> usize {
        let mut rx = 0;
        for j in 0..self.cx {
            if let Some(c) = row.chars().nth(j) {
                if c == '\t' {
                    rx += (KILO_TAB - 1) - (rx % KILO_TAB);
                }
                rx += 1;
            }
        }
        rx
    }

    fn scroll(&mut self) {
        self.rx = 0;
        if self.cy < self.num_rows {
            self.rx = self.row_cx_to_rx(&self.row[self.cy]);
        }

        if self.cy < self.row_off {
            self.row_off = self.cy;
        }
        if self.cy >= self.row_off + self.screen_rows {
            self.row_off = self.cy - self.screen_rows + 1;
        }
        if self.rx < self.col_off {
            self.col_off = self.rx;
        }
        if self.rx >= self.col_off + self.screen_cols {
            self.col_off = self.rx - self.screen_cols + 1;
        }
    }

    fn editor_draw_rows(&mut self) {
        for y in 0..self.screen_rows {
            let filerow = y + self.row_off;
            if filerow >= self.num_rows {
                if self.num_rows == 0 && y == self.screen_rows / 3 {
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
            } else {
                let row = &self.render[filerow];
                let mut start = 0;
                let mut end = row.len();

                for (i, (byte_idx, _)) in row.char_indices().enumerate() {
                    if i == self.col_off {
                        start = byte_idx;
                    }
                    if i == self.col_off + self.screen_cols {
                        end = byte_idx;
                        break;
                    }
                }

                self.buffer.push_str(&row[start..end]);
            }

            self.buffer.push_str("\x1b[K");
            self.buffer.push_str("\r\n");
        }
    }

    fn draw_status_bar(&mut self) {
        self.buffer.push_str("\x1b[7m"); // Inverted colors
        let mut filename = String::new();
        if self.filename.is_empty() {
            filename.push_str("[No Name]");
        } else {
            filename.push_str(self.filename.as_str());
        }

        let status = format!("{:.20} - {}", filename, self.num_rows);
        let rstatus = format!("{}/{}", self.cy + 1, self.num_rows);
        let mut len = status.len();
        if len > self.screen_cols {
            len = self.screen_cols;
        }
        self.buffer.push_str(&status[..len]);

        while len < self.screen_cols {
            if self.screen_cols - len == rstatus.len() {
                self.buffer.push_str(rstatus.as_str());
                break;
            } else {
                self.buffer.push(' ');
                len += 1;
            }
        }
        self.buffer.push_str("\x1b[m"); // Switches back to normal colors
    }

    fn refresh_screen(&mut self) {
        self.scroll();

        self.buffer.clear();
        self.buffer.push_str("\x1b[?25l");
        self.buffer.push_str("\x1b[H");

        self.editor_draw_rows();
        self.draw_status_bar();

        let cursor_position = format!(
            "\x1b[{};{}H",
            (self.cy - self.row_off) + 1,
            (self.rx - self.col_off) + 1,
        );
        self.buffer.push_str(cursor_position.as_str());

        self.buffer.push_str("\x1b[?25h");
        write_stdout(self.buffer.as_bytes());
    }

    pub fn open(&mut self, filename: &str) {
        if let Ok(file) = File::open(filename) {
            self.filename = filename.to_string();
            let reader = io::BufReader::new(file);

            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        self.row.push(line);
                        self.num_rows += 1;
                    }
                    Err(e) => {
                        eprintln!("Error while reading file: {}", e);
                    }
                }
            }
        } else {
            eprintln!("Couldn't open file: {}", filename);
        }

        for (i, line) in self.row.iter().enumerate() {
            self.render.push(String::new());
            for char in line.chars() {
                if char == '\t' {
                    for _ in 0..KILO_TAB {
                        self.render[i].push(' ');
                    }
                } else {
                    self.render[i].push(char);
                }
            }
        }
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

    let args: Vec<_> = env::args().collect();
    if args.len() >= 2 {
        editor.open(&args[1]);
    }

    editor.run();
}
