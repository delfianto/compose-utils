use regex::Regex;
use std::sync::LazyLock;

/// Regex for stripping ANSI escape codes from strings.
static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap());

/// Simple table formatter for CLI output.
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<&str>) -> Self {
        Self {
            headers: headers.into_iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return "".to_string();
        }

        let num_cols = self.headers.len();

        let mut col_widths = vec![0; num_cols];

        for (i, header) in self.headers.iter().enumerate() {
            col_widths[i] = header.len();
        }

        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                for line in cell.lines() {
                    let stripped = self.strip_ansi(line);

                    let display_width = stripped.chars().count();

                    if display_width > col_widths[i] {
                        col_widths[i] = display_width;
                    }
                }
            }
        }

        let mut output = String::new();

        for (i, header) in self.headers.iter().enumerate() {
            output.push_str(&format!("{:<width$}  ", header, width = col_widths[i]));
        }

        output.push('\n');

        for width in &col_widths {
            output.push_str(&"-".repeat(*width));

            output.push_str("  ");
        }

        output.push('\n');

        for row in &self.rows {
            let mut max_lines = 0;

            let mut cell_lines: Vec<Vec<&str>> = Vec::new();

            for cell in row {
                let lines: Vec<&str> = cell.lines().collect();

                if lines.len() > max_lines {
                    max_lines = lines.len();
                }

                cell_lines.push(lines);
            }

            for line_idx in 0..max_lines {
                for (col_idx, lines) in cell_lines.iter().enumerate() {
                    let line = lines.get(line_idx).unwrap_or(&"");

                    let stripped = self.strip_ansi(line);

                    let display_width = stripped.chars().count();

                    let padding = col_widths[col_idx] - display_width;

                    output.push_str(line);

                    output.push_str(&" ".repeat(padding));

                    output.push_str("  ");
                }

                output.push('\n');
            }
        }

        output
    }

    fn strip_ansi(&self, s: &str) -> String {
        ANSI_RE.replace_all(s, "").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_table_renders_empty_string() {
        let table = Table::new(vec!["A", "B", "C"]);
        assert_eq!(table.render(), "");
    }

    #[test]
    fn test_single_row() {
        let mut table = Table::new(vec!["NAME", "VALUE"]);
        table.add_row(vec!["foo".into(), "bar".into()]);
        let output = table.render();
        assert!(output.contains("NAME"));
        assert!(output.contains("VALUE"));
        assert!(output.contains("foo"));
        assert!(output.contains("bar"));
        // Should have header line, separator line, data line
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_multiple_rows() {
        let mut table = Table::new(vec!["ID", "NAME"]);
        table.add_row(vec!["1".into(), "alice".into()]);
        table.add_row(vec!["2".into(), "bob".into()]);
        table.add_row(vec!["3".into(), "charlie".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        // header + separator + 3 data rows
        assert_eq!(lines.len(), 5);
        assert!(output.contains("charlie"));
    }

    #[test]
    fn test_column_width_adapts_to_data() {
        let mut table = Table::new(vec!["X"]);
        table.add_row(vec!["very_long_value".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        // Separator dashes should be at least as long as "very_long_value"
        let separator = lines[1];
        assert!(separator.trim().len() >= "very_long_value".len());
    }

    #[test]
    fn test_column_width_adapts_to_header() {
        let mut table = Table::new(vec!["LONG_HEADER_NAME"]);
        table.add_row(vec!["x".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        let separator = lines[1];
        assert!(separator.trim().len() >= "LONG_HEADER_NAME".len());
    }

    #[test]
    fn test_multiline_cell() {
        let mut table = Table::new(vec!["PORTS", "NAME"]);
        table.add_row(vec!["8080/tcp\n443/tcp".into(), "web".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        // header + separator + 2 lines for the multiline cell
        assert_eq!(lines.len(), 4);
        assert!(output.contains("8080/tcp"));
        assert!(output.contains("443/tcp"));
    }

    #[test]
    fn test_multiline_different_heights() {
        let mut table = Table::new(vec!["A", "B"]);
        table.add_row(vec!["line1\nline2\nline3".into(), "single".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        // header + separator + 3 lines for the tallest cell
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_strip_ansi_codes() {
        let table = Table::new(vec!["X"]);
        let stripped = table.strip_ansi("\x1B[32mgreen\x1B[0m");
        assert_eq!(stripped, "green");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let table = Table::new(vec!["X"]);
        let stripped = table.strip_ansi("plain text");
        assert_eq!(stripped, "plain text");
    }

    #[test]
    fn test_strip_ansi_multiple_codes() {
        let table = Table::new(vec!["X"]);
        let stripped = table.strip_ansi("\x1B[1m\x1B[31mbold red\x1B[0m");
        assert_eq!(stripped, "bold red");
    }

    #[test]
    fn test_ansi_does_not_affect_width_calculation() {
        let mut table = Table::new(vec!["STATUS"]);
        // "ok" is 2 chars, but with ANSI codes the raw string is much longer
        table.add_row(vec!["\x1B[32mok\x1B[0m".into()]);
        let output = table.render();
        // The separator line should be based on header width ("STATUS" = 6), not raw string
        let lines: Vec<&str> = output.lines().collect();
        let separator = lines[1].trim_end();
        // dashes should equal "STATUS" width (6) + trailing spaces
        assert!(separator.starts_with("------"));
    }

    #[test]
    fn test_separator_line_format() {
        let mut table = Table::new(vec!["AB", "CDE"]);
        table.add_row(vec!["x".into(), "y".into()]);
        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();
        let separator = lines[1];
        // Should contain dashes for each column
        assert!(separator.contains("--"));
    }

    #[test]
    fn test_empty_cell_value() {
        let mut table = Table::new(vec!["A", "B"]);
        table.add_row(vec!["".into(), "data".into()]);
        let output = table.render();
        assert!(output.contains("data"));
        // Should not crash on empty cell
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_unicode_in_cells() {
        let mut table = Table::new(vec!["NAME"]);
        table.add_row(vec!["caf\u{00e9}".into()]);
        let output = table.render();
        assert!(output.contains("caf\u{00e9}"));
    }
}
