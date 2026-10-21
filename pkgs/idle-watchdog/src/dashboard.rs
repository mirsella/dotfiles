use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::{Duration, Instant};

use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table, presets::UTF8_FULL};

use crate::runtime::{Cancellation, command_display};

pub struct DashboardRow {
    pub status: String,
    pub color: Color,
    pub age: String,
    pub updated: String,
    pub scope: String,
    pub id: String,
    pub title: String,
}

pub struct DashboardView {
    pub status: String,
    pub rows: Vec<DashboardRow>,
    pub empty: (String, Color, String),
    pub extra: Option<String>,
}

enum Key {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Ignore,
}

pub struct Dashboard {
    termios: Option<libc::termios>,
    action: Option<String>,
    title: &'static str,
    loading: &'static str,
    headers: [&'static str; 6],
    terminal_size: std::cell::Cell<(u16, u16)>,
    scroll: std::cell::Cell<usize>,
    page: std::cell::Cell<usize>,
    max_scroll: std::cell::Cell<usize>,
}

impl Dashboard {
    fn terminal_size() -> (u16, u16) {
        let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0 {
            (
                if size.ws_col > 0 { size.ws_col } else { 80 },
                if size.ws_row > 0 { size.ws_row } else { 24 },
            )
        } else {
            (80, 24)
        }
    }

    pub fn new(
        command: &[OsString],
        title: &'static str,
        loading: &'static str,
        headers: [&'static str; 6],
    ) -> Self {
        let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        let termios =
            if interactive && unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } == 0 {
                let mut cbreak = original;
                cbreak.c_lflag &= !(libc::ICANON | libc::ECHO);
                cbreak.c_cc[libc::VMIN] = 0;
                cbreak.c_cc[libc::VTIME] = 0;
                (unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &cbreak) } == 0)
                    .then_some(original)
            } else {
                None
            };
        if termios.is_some() {
            print!("\x1b[?1049h\x1b[?25l\x1b[?7l");
            let _ = io::stdout().flush();
        }
        let action = (!command.is_empty()).then(|| command_display(command));
        Self {
            termios,
            action,
            title,
            loading,
            headers,
            terminal_size: std::cell::Cell::new(Self::terminal_size()),
            scroll: std::cell::Cell::new(0),
            page: std::cell::Cell::new(1),
            max_scroll: std::cell::Cell::new(0),
        }
    }

    pub fn termios_active(&self) -> bool {
        self.termios.is_some()
    }

    fn render(&self, frame: &str) {
        if self.termios.is_none() {
            return;
        }
        let mut redraw = String::new();
        for (row, line) in frame
            .lines()
            .take(usize::from(self.terminal_size.get().1))
            .enumerate()
        {
            write!(redraw, "\x1b[{};1H\x1b[2K{line}", row + 1)
                .expect("writing to a string cannot fail");
        }
        redraw.push_str("\x1b[J\x1b[H");
        print!("{redraw}");
        let _ = io::stdout().flush();
    }

    pub fn loading(&self) {
        self.render(&format!(
            "\x1b[1;36m{}\x1b[0m\n\n\x1b[2m{}\x1b[0m",
            self.title, self.loading
        ));
    }

    fn read_byte() -> Option<u8> {
        let mut byte = 0_u8;
        (unsafe { libc::read(libc::STDIN_FILENO, std::ptr::from_mut(&mut byte).cast(), 1) } == 1)
            .then_some(byte)
    }

    fn stdin_ready(timeout_ms: i32) -> bool {
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut descriptor, 1, timeout_ms) > 0 }
    }

    fn read_key() -> Option<Key> {
        match Self::read_byte()? {
            b'q' => Some(Key::Quit),
            b'k' => Some(Key::Up),
            b'j' => Some(Key::Down),
            0x1b => {
                if !Self::stdin_ready(10) {
                    return Some(Key::Ignore);
                }
                let Some(second) = Self::read_byte() else {
                    return Some(Key::Ignore);
                };
                if !matches!(second, b'[' | b'O') {
                    return Some(Key::Ignore);
                }
                if !Self::stdin_ready(10) {
                    return Some(Key::Ignore);
                }
                let Some(third) = Self::read_byte() else {
                    return Some(Key::Ignore);
                };
                match third {
                    b'A' => Some(Key::Up),
                    b'B' => Some(Key::Down),
                    b'H' => Some(Key::Home),
                    b'F' => Some(Key::End),
                    b'1'..=b'9' => {
                        if !Self::stdin_ready(10) {
                            return Some(Key::Ignore);
                        }
                        let Some(fourth) = Self::read_byte() else {
                            return Some(Key::Ignore);
                        };
                        match (third, fourth) {
                            (b'1', b'~') | (b'7', b'~') => Some(Key::Home),
                            (b'4', b'~') | (b'8', b'~') => Some(Key::End),
                            (b'5', b'~') => Some(Key::PageUp),
                            (b'6', b'~') => Some(Key::PageDown),
                            _ => Some(Key::Ignore),
                        }
                    }
                    _ => Some(Key::Ignore),
                }
            }
            _ => Some(Key::Ignore),
        }
    }

    fn scroll_by(&self, delta: isize) -> bool {
        let current = self.scroll.get();
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(self.max_scroll.get());
        if next == current {
            false
        } else {
            self.scroll.set(next);
            true
        }
    }

    pub fn wait(
        &self,
        cancellation: &Cancellation,
        duration: Duration,
        mut on_redraw: impl FnMut(),
    ) -> Option<i32> {
        if self.termios.is_none() {
            return cancellation.wait(duration);
        }
        let started = Instant::now();
        loop {
            if let Some(key) = Self::read_key() {
                match key {
                    Key::Quit => return Some(0),
                    Key::Up => {
                        if self.scroll_by(-1) {
                            on_redraw();
                        }
                    }
                    Key::Down => {
                        if self.scroll_by(1) {
                            on_redraw();
                        }
                    }
                    Key::PageUp => {
                        let page = self.page.get().max(1) as isize;
                        if self.scroll_by(-page) {
                            on_redraw();
                        }
                    }
                    Key::PageDown => {
                        let page = self.page.get().max(1) as isize;
                        if self.scroll_by(page) {
                            on_redraw();
                        }
                    }
                    Key::Home => {
                        if self.scroll.get() != 0 {
                            self.scroll.set(0);
                            on_redraw();
                        }
                    }
                    Key::End => {
                        let end = self.max_scroll.get();
                        if self.scroll.get() != end {
                            self.scroll.set(end);
                            on_redraw();
                        }
                    }
                    Key::Ignore => {}
                }
            }
            let terminal_size = Self::terminal_size();
            if terminal_size != self.terminal_size.replace(terminal_size) {
                on_redraw();
            }
            let remaining = duration.saturating_sub(started.elapsed());
            if let Some(signal) = cancellation.wait(remaining.min(Duration::from_millis(25))) {
                return Some(signal);
            }
            if started.elapsed() >= duration {
                return None;
            }
        }
    }

    pub fn run_work<T: Send + 'static>(
        &self,
        cancellation: &Cancellation,
        mut on_redraw: impl FnMut(),
        work: impl FnOnce() -> T + Send + 'static,
    ) -> std::result::Result<T, i32> {
        let worker = thread::spawn(work);
        while !worker.is_finished() {
            if let Some(signal) = self.wait(cancellation, Duration::from_millis(10), &mut on_redraw)
            {
                return Err(signal);
            }
        }
        Ok(worker.join().expect("scan worker panicked"))
    }

    pub fn tick(&self, status: &str) {
        if self.termios.is_some() {
            print!("\x1b[2;1H\x1b[2K{status}\x1b[H");
            let _ = io::stdout().flush();
        }
    }

    pub fn draw(&self, view: DashboardView) {
        if self.termios.is_none() {
            return;
        }
        let width = self.terminal_size.get().0;
        let mut table = Table::new();
        table
            .load_style(UTF8_FULL.with_rounded_corners().with_solid_inner_borders())
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_width(width)
            .set_header(self.headers);
        for column in [1, 2] {
            table
                .column_mut(column)
                .expect("age and updated columns exist")
                .set_cell_alignment(CellAlignment::Right);
        }
        if view.rows.is_empty() {
            let (label, color, message) = view.empty;
            table.add_row([
                Cell::new(label).fg(color),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(message),
            ]);
        }
        for row in view.rows {
            table.add_row([
                Cell::new(row.status).fg(row.color),
                Cell::new(row.age),
                Cell::new(row.updated),
                Cell::new(row.scope),
                Cell::new(row.id),
                Cell::new(row.title),
            ]);
        }
        let mut body = table.to_string();
        if let Some(extra) = view.extra {
            body.push_str("\n\n");
            body.push_str(&extra);
        }
        let body_lines: Vec<&str> = body.lines().collect();
        let mut top = format!("\x1b[1;36m{}\x1b[0m\n{}\n", self.title, view.status);
        if let Some(action) = &self.action {
            writeln!(top, "\x1b[2mon idle       |\x1b[0m  {action}")
                .expect("writing to a string cannot fail");
        }
        top.push('\n');

        // Keep the title, status and footer fixed; scroll the table and API
        // errors through the remaining terminal rows.
        let height = usize::from(self.terminal_size.get().1);
        let top_height = top.lines().count();
        let footer_height = 3;
        let available = height.saturating_sub(top_height + footer_height);
        let max_scroll = body_lines.len().saturating_sub(available);
        let scroll = self.scroll.get().min(max_scroll);
        let visible_end = (scroll + available).min(body_lines.len());
        self.scroll.set(scroll);
        self.page.set(available.max(1));
        self.max_scroll.set(max_scroll);

        let mut footer = String::from("\x1b[2mq / Ctrl-C to exit");
        if max_scroll > 0 {
            write!(
                footer,
                "  |  lines {}-{}/{}  |  ↑↓ PgUp/PgDn Home/End",
                scroll + 1,
                visible_end,
                body_lines.len()
            )
            .expect("writing to a string cannot fail");
        } else {
            footer.push_str("  |  ↑↓ PgUp/PgDn Home/End");
        }
        footer.push_str("\x1b[0m");

        let mut frame: Vec<String> = top.lines().map(str::to_owned).collect();
        frame.extend(
            body_lines[scroll..visible_end]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        // Keep the help footer on the last rows even when the table is short.
        let padding = height.saturating_sub(frame.len() + footer_height);
        for _ in 0..padding {
            frame.push(String::new());
        }
        frame.push(String::new());
        frame.push(String::new());
        frame.push(footer);
        self.render(&frame.join("\n"));
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(original) = &self.termios {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original);
            }
        }
        if self.termios.is_some() {
            print!("\x1b[?7h\x1b[?25h\x1b[?1049l");
            let _ = io::stdout().flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_scrolls_within_bounds() {
        let dashboard = Dashboard {
            termios: None,
            action: None,
            title: "TEST",
            loading: "Loading",
            headers: ["", "", "", "", "", ""],
            terminal_size: std::cell::Cell::new((80, 24)),
            scroll: std::cell::Cell::new(0),
            page: std::cell::Cell::new(4),
            max_scroll: std::cell::Cell::new(10),
        };
        assert!(dashboard.scroll_by(3));
        assert_eq!(dashboard.scroll.get(), 3);
        assert!(dashboard.scroll_by(-10));
        assert_eq!(dashboard.scroll.get(), 0);
        assert!(dashboard.scroll_by(100));
        assert_eq!(dashboard.scroll.get(), 10);
        assert!(!dashboard.scroll_by(1));
        assert_eq!(dashboard.page.get(), 4);
    }
}
