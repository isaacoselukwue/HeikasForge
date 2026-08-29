use std::io::IsTerminal;

use heikas_domain::candidate::CandidateStatus;
use heikas_domain::run::RunStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    enabled: bool,
}

impl Palette {
    pub fn detect(force_plain: bool) -> Self {
        let enabled =
            !force_plain && std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err();
        Self { enabled }
    }

    pub fn plain() -> Self {
        Self { enabled: false }
    }

    fn wrap(&self, code: &str, value: &str) -> String {
        if self.enabled {
            format!("\u{1b}[{code}m{value}\u{1b}[0m")
        } else {
            value.to_string()
        }
    }

    pub fn heading(&self, value: &str) -> String {
        self.wrap("1;36", value)
    }

    pub fn success(&self, value: &str) -> String {
        self.wrap("32", value)
    }

    pub fn warning(&self, value: &str) -> String {
        self.wrap("33", value)
    }

    pub fn failure(&self, value: &str) -> String {
        self.wrap("31", value)
    }

    pub fn muted(&self, value: &str) -> String {
        self.wrap("90", value)
    }

    pub fn run_status(&self, status: RunStatus) -> String {
        match status {
            RunStatus::Succeeded => self.success(status.label()),
            RunStatus::Failed | RunStatus::Exhausted => self.failure(status.label()),
            RunStatus::Cancelled | RunStatus::RecoveryRequired => self.warning(status.label()),
            RunStatus::AwaitingPlanApproval | RunStatus::AwaitingCommitApproval => {
                self.warning(status.label())
            }
            _ => status.label().to_string(),
        }
    }

    pub fn candidate_status(&self, status: CandidateStatus) -> String {
        match status {
            CandidateStatus::Eligible => self.success(status.label()),
            CandidateStatus::Ineligible | CandidateStatus::Cancelled => {
                self.failure(status.label())
            }
            CandidateStatus::Interrupted => self.warning(status.label()),
            _ => status.label().to_string(),
        }
    }
}

pub struct Table {
    headings: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headings: &[&str]) -> Self {
        Self {
            headings: headings.iter().map(|value| (*value).to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn render(&self, palette: &Palette) -> String {
        let column_count = self.headings.len();
        let mut widths = vec![0usize; column_count];
        for (index, heading) in self.headings.iter().enumerate() {
            widths[index] = display_width(heading);
        }
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate().take(column_count) {
                widths[index] = widths[index].max(display_width(cell));
            }
        }
        let mut output = String::new();
        for (index, heading) in self.headings.iter().enumerate() {
            output.push_str(&palette.heading(&pad(heading, widths[index])));
            if index + 1 < column_count {
                output.push_str("  ");
            }
        }
        output.push('\n');
        for (index, width) in widths.iter().enumerate() {
            output.push_str(&palette.muted(&"-".repeat(*width)));
            if index + 1 < column_count {
                output.push_str("  ");
            }
        }
        output.push('\n');
        for row in &self.rows {
            for (index, width) in widths.iter().enumerate() {
                let cell = row.get(index).map(String::as_str).unwrap_or("");
                output.push_str(&pad(cell, *width));
                if index + 1 < column_count {
                    output.push_str("  ");
                }
            }
            output.push('\n');
        }
        output
    }
}

fn pad(value: &str, width: usize) -> String {
    let current = display_width(value);
    if current >= width {
        value.to_string()
    } else {
        format!("{value}{}", " ".repeat(width - current))
    }
}

fn display_width(value: &str) -> usize {
    let mut width = 0usize;
    let mut in_escape = false;
    for character in value.chars() {
        if in_escape {
            if character == 'm' {
                in_escape = false;
            }
            continue;
        }
        if character == '\u{1b}' {
            in_escape = true;
            continue;
        }
        width += 1;
    }
    width
}
