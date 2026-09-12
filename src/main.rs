use std::env;
use std::fs;
use std::process::ExitCode;

// USPS guidance for printed address blocks is to keep each line at or under
// 40 characters so it fits a standard label and reads cleanly through OCR.
const MAX_LINE_LEN: usize = 40;

const STATE_ABBREVS: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA",
    "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ",
    "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT",
    "VA", "WA", "WV", "WI", "WY", "DC", "PR", "VI", "GU", "AS", "MP",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

struct Finding {
    line: usize,
    rule: &'static str,
    severity: Severity,
    message: String,
}

fn lint(text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        check_line(idx + 1, line, &mut findings);
    }

    let mut block_start = 0usize;
    let mut in_block = false;
    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if line.trim().is_empty() {
            if in_block {
                check_block(block_start, &lines[block_start - 1..idx], &mut findings);
                in_block = false;
            }
        } else if !in_block {
            block_start = line_no;
            in_block = true;
        }
    }
    if in_block {
        check_block(block_start, &lines[block_start - 1..], &mut findings);
    }

    findings.sort_by_key(|f| f.line);
    findings
}

fn check_line(line_no: usize, line: &str, findings: &mut Vec<Finding>) {
    let len = line.chars().count();
    if len > MAX_LINE_LEN {
        findings.push(Finding {
            line: line_no,
            rule: "line-too-long",
            severity: Severity::Warning,
            message: format!(
                "line is {} characters, over the {}-character limit for a printable address line",
                len, MAX_LINE_LEN
            ),
        });
    }
    if line != line.trim_end() {
        findings.push(Finding {
            line: line_no,
            rule: "trailing-whitespace",
            severity: Severity::Warning,
            message: "line has trailing whitespace".to_string(),
        });
    }
}

// A block is a run of non-blank lines, e.g. a name plus a street line plus a
// city/state/zip line. We only inspect the last line for state and zip since
// that is where USPS expects them; earlier lines are free-form street/unit info.
fn check_block(start_line: usize, block: &[&str], findings: &mut Vec<Finding>) {
    if block.len() < 2 {
        findings.push(Finding {
            line: start_line,
            rule: "incomplete-block",
            severity: Severity::Error,
            message: format!(
                "address block has only {} line(s); expected at least a street line and a city/state/zip line",
                block.len()
            ),
        });
        return;
    }

    let last_idx = block.len() - 1;
    let last_line = block[last_idx];
    let last_line_no = start_line + last_idx;
    let tokens: Vec<&str> = last_line.split_whitespace().collect();

    let state_token = tokens.iter().find_map(|t| {
        let cleaned = t.trim_matches(|c: char| !c.is_ascii_alphabetic());
        if cleaned.len() == 2 && STATE_ABBREVS.contains(&cleaned.to_ascii_uppercase().as_str()) {
            Some(cleaned)
        } else {
            None
        }
    });

    match state_token {
        None => findings.push(Finding {
            line: last_line_no,
            rule: "missing-state",
            severity: Severity::Error,
            message: "last line of address block has no recognizable state or territory code"
                .to_string(),
        }),
        Some(s) if s.chars().any(|c| c.is_ascii_lowercase()) => findings.push(Finding {
            line: last_line_no,
            rule: "state-not-uppercase",
            severity: Severity::Warning,
            message: format!("state code '{}' should be uppercase", s),
        }),
        Some(_) => {}
    }

    if !tokens.iter().any(|t| is_zip(t)) {
        findings.push(Finding {
            line: last_line_no,
            rule: "missing-zip",
            severity: Severity::Error,
            message: "last line of address block has no recognizable ZIP code (5 digits, optionally followed by -4)".to_string(),
        });
    }
}

fn is_zip(token: &str) -> bool {
    let cleaned = token.trim_matches(|c: char| c == ',' || c == '.');
    let chars: Vec<char> = cleaned.chars().collect();
    match chars.len() {
        5 => chars.iter().all(|c| c.is_ascii_digit()),
        10 => {
            chars[5] == '-'
                && chars[..5].iter().all(|c| c.is_ascii_digit())
                && chars[6..].iter().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn print_json(path: &str, findings: &[Finding]) {
    let mut out = String::from("[");
    for (i, f) in findings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"file\":\"{}\",\"line\":{},\"rule\":\"{}\",\"severity\":\"{}\",\"message\":\"{}\"}}",
            json_escape(path),
            f.line,
            f.rule,
            f.severity.as_str(),
            json_escape(&f.message)
        ));
    }
    out.push(']');
    println!("{}", out);
}

fn print_human(path: &str, findings: &[Finding]) {
    if findings.is_empty() {
        println!("{}: no issues found", path);
        return;
    }
    for f in findings {
        println!(
            "{}:{}: {}: {} [{}]",
            path,
            f.line,
            f.severity.as_str(),
            f.message,
            f.rule
        );
    }
    let errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count();
    println!("{} error(s), {} warning(s)", errors, warnings);
}

fn print_usage() {
    eprintln!("usage: addrlint <file> [--json]");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut json_output = false;
    let mut path: Option<String> = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "--json" => json_output = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => path = Some(other.to_string()),
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("addrlint: no input file given");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("addrlint: cannot read {}: {}", path, e);
            return ExitCode::from(2);
        }
    };

    let findings = lint(&text);
    let has_errors = findings.iter().any(|f| f.severity == Severity::Error);

    if json_output {
        print_json(&path, &findings);
    } else {
        print_human(&path, &findings);
    }

    if has_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
