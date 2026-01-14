use std::io::{IsTerminal, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::errors::CliError;

enum Entry {
    Value(serde_json::Value),
    Sublist(StructuredPrint),
}

#[derive(Default)]
pub struct StructuredPrint {
    entries: Vec<(String, Entry)>,
    no_align: bool,
}

impl StructuredPrint {
    pub fn sublist(&mut self, key: impl ToString, f: impl FnOnce(&mut StructuredPrint)) {
        let mut obj = StructuredPrint::default();
        f(&mut obj);
        self.entries.push((key.to_string(), Entry::Sublist(obj)))
    }
    pub fn key_value(&mut self, key: impl ToString, value: impl Into<serde_json::Value>) {
        self.entries
            .push((key.to_string(), Entry::Value(value.into())))
    }
    pub fn key_value_maybe<T: Into<serde_json::Value>>(
        &mut self,
        key: impl ToString,
        value: Option<T>,
    ) {
        if let Some(value) = value {
            self.key_value(key, value);
        }
    }

    pub fn unaligned(&mut self) {
        self.no_align = true;
    }

    pub fn print(self, depth: usize) {
        self.print_impl(
            depth,
            &mut StandardStream::stdout(if std::io::stdout().is_terminal() {
                ColorChoice::Auto
            } else {
                ColorChoice::Never
            }),
        );
    }

    fn print_impl(self, depth: usize, stdout: &mut StandardStream) {
        let indent = std::iter::repeat_n("    ", depth).collect::<String>();
        let longest_key = self
            .entries
            .iter()
            .map(|(key, _)| key.len())
            .max()
            .unwrap_or(0);

        for (key, value) in self.entries {
            if depth == 0 {
                writeln!(stdout).ok();
            }
            let padding = std::iter::repeat_n(
                ' ',
                if self.no_align {
                    1
                } else {
                    (longest_key + 1) - key.len()
                },
            )
            .collect::<String>();
            match value {
                Entry::Value(value) => {
                    let (value, color) = match value {
                        serde_json::Value::Null => ("---".to_string(), None),
                        serde_json::Value::Bool(val) => (
                            val.to_string(),
                            Some(if val { Color::Green } else { Color::Red }),
                        ),
                        serde_json::Value::Number(number) => (number.to_string(), None),
                        serde_json::Value::String(s) => (s, None),
                        serde_json::Value::Array(_) => ("...".to_string(), None),
                        serde_json::Value::Object(_) => ("...".to_string(), None),
                    };

                    write!(stdout, "{}{}:{}", indent, key, padding).ok();
                    if let Some(color) = color {
                        stdout.set_color(ColorSpec::new().set_fg(Some(color))).ok();
                        writeln!(stdout, "{}", value).ok();
                        stdout.reset().ok();
                    } else {
                        writeln!(stdout, "{}", value).ok();
                    }
                }
                Entry::Sublist(sublist) => {
                    writeln!(stdout, "{}{}:", indent, key).ok();
                    sublist.print_impl(depth + 1, stdout);
                }
            }
        }
        if depth == 0 {
            writeln!(stdout).ok();
        }
    }

    fn collect_json(self) -> serde_json::Map<String, serde_json::Value> {
        let mut val = serde_json::Map::new();

        for (key, value) in self.entries {
            let value = match value {
                Entry::Value(value) => value,
                Entry::Sublist(sublist) => sublist.collect_json().into(),
            };

            val.insert(key, value);
        }

        val
    }

    pub fn print_json(self) -> Result<(), CliError> {
        let json_str = serde_json::to_string_pretty(&self.collect_json())
            .map_err(CliError::JsonEncodeError)?;
        println!("{json_str}");
        Ok(())
    }
}

pub fn structured_print(
    header: Option<String>,
    json: bool,
    f: impl FnOnce(&mut StructuredPrint),
) -> Result<(), CliError> {
    let mut obj = StructuredPrint::default();

    if let Some(header) = header {
        if json {
            f(&mut obj);
        } else {
            obj.sublist(header, f);
        }
    } else {
        f(&mut obj);
    }
    if json {
        obj.print_json()?;
    } else {
        obj.print(0);
    }
    Ok(())
}
