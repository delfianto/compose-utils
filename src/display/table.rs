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
        let re = regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap();
        re.replace_all(s, "").to_string()
    }
}