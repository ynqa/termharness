use std::io::{self, Write};

use super::Run;

pub fn format_run(run: &Run) -> String {
    let mut output = String::new();

    for (index, record) in run.records.iter().enumerate() {
        output.push_str(&format_header(&record.label));

        for (row, line) in record.screen.iter().enumerate() {
            output.push_str(&format_line(row, line));
        }

        if index + 1 != run.records.len() {
            output.push('\n');
        }
    }

    output
}

pub fn write_to<W: Write>(run: &Run, mut writer: W) -> io::Result<()> {
    writer.write_all(format_run(run).as_bytes())
}

fn format_header(label: &str) -> String {
    format!("== {label} ==\n")
}

fn format_line(row: usize, line: &str) -> String {
    format!("  r{row:02} {}\n", format_screen_line(line))
}

fn format_screen_line(line: &str) -> String {
    format!("|{}|", line.replace(' ', "·"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Record, Run};
    use indoc::indoc;

    mod format_run {
        use super::*;

        #[test]
        fn formats_records_as_snapshot_text() {
            let run = Run {
                records: vec![
                    Record {
                        label: "type text".to_string(),
                        screen: vec!["hello      ".to_string(), "world wide ".to_string()],
                    },
                    Record {
                        label: "insert text".to_string(),
                        screen: vec!["hello again".to_string()],
                    },
                ],
            };

            assert_eq!(
                format_run(&run),
                indoc! {"
                    == type text ==
                      r00 |hello······|
                      r01 |world·wide·|

                    == insert text ==
                      r00 |hello·again|
                "}
            );
        }
    }

    mod format_header {
        #[test]
        fn formats_record_label() {
            assert_eq!(super::format_header("step"), "== step ==\n");
        }
    }

    mod format_line {
        #[test]
        fn formats_row_and_screen_line() {
            assert_eq!(super::format_line(3, "a b"), "  r03 |a·b|\n");
        }
    }

    mod write_to {
        use super::*;

        #[test]
        fn writes_formatted_snapshot() {
            let run = Run {
                records: vec![Record {
                    label: "step".to_string(),
                    screen: vec!["a b  ".to_string()],
                }],
            };

            let mut output = Vec::new();
            write_to(&run, &mut output).expect("write should succeed");

            assert_eq!(
                String::from_utf8(output).expect("output should be utf-8"),
                indoc! {"
                    == step ==
                      r00 |a·b··|
                "}
            );
        }
    }
}
