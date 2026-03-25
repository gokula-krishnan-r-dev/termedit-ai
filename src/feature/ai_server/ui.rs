//! Terminal helpers: diff highlighting and sections.

pub fn print_section(title: &str) {
    println!("\n━━ {} ━━", title);
}

pub fn highlight_diff_line(line: &str) -> String {
    if line.starts_with('+') && !line.starts_with("+++") {
        format!("\x1b[32m{}\x1b[0m", line)
    } else if line.starts_with('-') && !line.starts_with("---") {
        format!("\x1b[31m{}\x1b[0m", line)
    } else if line.starts_with('@') {
        format!("\x1b[36m{}\x1b[0m", line)
    } else {
        line.to_string()
    }
}

pub fn print_diff(diff: &str) {
    for line in diff.lines() {
        println!("{}", highlight_diff_line(line));
    }
}
