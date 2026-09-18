use std::env;
use std::fs;
use std::io::{self, Read};
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
    eprintln!("usage: addrlint [<file>] [--json]");
    eprintln!("       omit <file> or pass - to read from stdin");
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

    let (label, text) = match path.as_deref() {
        None | Some("-") => {
            let mut buf = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut buf) {
                eprintln!("addrlint: cannot read stdin: {}", e);
                return ExitCode::from(2);
            }
            ("<stdin>".to_string(), buf)
        }
        Some(p) => match fs::read_to_string(p) {
            Ok(t) => (p.to_string(), t),
            Err(e) => {
                eprintln!("addrlint: cannot read {}: {}", p, e);
                return ExitCode::from(2);
            }
        },
    };

    let findings = lint(&text);
    let has_errors = findings.iter().any(|f| f.severity == Severity::Error);

    if json_output {
        print_json(&label, &findings);
    } else {
        print_human(&label, &findings);
    }

    if has_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn is_zip_accepts_five_digit() {
        assert!(is_zip("62704"));
    }

    #[test]
    fn is_zip_accepts_zip_plus_four() {
        assert!(is_zip("62704-1234"));
    }

    #[test]
    fn is_zip_accepts_trailing_punctuation() {
        assert!(is_zip("62704,"));
        assert!(is_zip("62704."));
    }

    #[test]
    fn is_zip_rejects_wrong_length() {
        assert!(!is_zip("6270"));
        assert!(!is_zip("627045"));
    }

    #[test]
    fn is_zip_rejects_non_digits() {
        assert!(!is_zip("6270A"));
        assert!(!is_zip("62704-12A4"));
    }

    #[test]
    fn is_zip_rejects_misplaced_dash() {
        assert!(!is_zip("627-041234"));
    }

    #[test]
    fn lint_clean_block_has_no_findings() {
        let text = "Maria Alvarez\n482 Cedarwood Lane\nSpringfield IL 62704\n";
        assert!(lint(text).is_empty());
    }

    #[test]
    fn lint_flags_long_line() {
        let text = "Name\n482 Cedarwood Lane Apartment 12B, Building C, Second Floor\nSpringfield IL 62704\n";
        let findings = lint(text);
        assert!(rules(&findings).contains(&"line-too-long"));
    }

    #[test]
    fn lint_flags_trailing_whitespace() {
        let text = "Name\n5 Maple Ct   \nAustin TX 78701\n";
        let findings = lint(text);
        assert!(rules(&findings).contains(&"trailing-whitespace"));
    }

    #[test]
    fn lint_flags_incomplete_block() {
        let text = "James Whitfield\n";
        let findings = lint(text);
        assert_eq!(rules(&findings), vec!["incomplete-block"]);
    }

    #[test]
    fn lint_flags_missing_state_and_zip() {
        let text = "James Whitfield\n19 Birch St\nPortland\n";
        let findings = lint(text);
        assert!(rules(&findings).contains(&"missing-state"));
        assert!(rules(&findings).contains(&"missing-zip"));
    }

    #[test]
    fn lint_flags_lowercase_state() {
        let text = "Tomoko Sato\n77 Harbor View Road\nPortland or 97201\n";
        let findings = lint(text);
        assert_eq!(rules(&findings), vec!["state-not-uppercase"]);
    }

    #[test]
    fn lint_reports_findings_in_line_order() {
        let text = "Name\n5 Maple Ct   \nAustin TX 78701\n\nOther\n19 Birch St\n";
        let findings = lint(text);
        let line_numbers: Vec<usize> = findings.iter().map(|f| f.line).collect();
        let mut sorted = line_numbers.clone();
        sorted.sort();
        assert_eq!(line_numbers, sorted);
    }

    #[test]
    fn lint_ignores_blank_lines_between_blocks() {
        let text = "Maria Alvarez\n482 Cedarwood Lane\nSpringfield IL 62704\n\nJames Whitfield\n19 Birch St\nPortland OR 97201\n";
        assert!(lint(text).is_empty());
    }
}
