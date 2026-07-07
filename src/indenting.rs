use std::fmt::{self, Write};

pub struct IndentedWriter<W: Write> {
    inner: W,
    indent: u32,
    indent_str: &'static str,
    at_line_start: bool,
}

impl<W: Write> IndentedWriter<W> {
    pub fn new(inner: W, indent_str: &'static str) -> Self {
        Self {
            inner,
            indent: 0,
            indent_str,
            at_line_start: true,
        }
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    /// The current indentation depth (number of `indent_str` prefixes written
    /// at each line start).
    pub fn depth(&self) -> u32 {
        self.indent
    }

    fn write_indent(&mut self) -> fmt::Result {
        for _ in 0..self.indent {
            self.inner.write_str(self.indent_str)?;
        }
        Ok(())
    }
}

impl<W: Write> Write for IndentedWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut start = 0;

        for (i, ch) in s.char_indices() {
            if self.at_line_start {
                self.write_indent()?;
                self.at_line_start = false;
            }

            if ch == '\n' {
                // write up to and including the newline
                self.inner.write_str(&s[start..=i])?;
                self.at_line_start = true;
                start = i + 1;
            }
        }

        if start < s.len() {
            self.inner.write_str(&s[start..])?;
        }

        Ok(())
    }
}
