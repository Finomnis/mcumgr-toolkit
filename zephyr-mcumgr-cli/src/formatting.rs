use crate::CliError;

enum Entry {
    Value(serde_json::Value),
    Sublist(StructuredPrint),
}

#[derive(Default)]
pub struct StructuredPrint {
    entries: Vec<(String, Entry)>,
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
    pub fn print(self, depth: usize) {
        let indent = std::iter::repeat_n("    ", depth).collect::<String>();
        let longest_key = self
            .entries
            .iter()
            .map(|(key, _)| key.len())
            .max()
            .unwrap_or(0);

        for (key, value) in self.entries {
            if depth == 0 {
                println!();
            }
            let padding =
                std::iter::repeat_n(' ', (longest_key + 1) - key.len()).collect::<String>();
            match value {
                Entry::Value(value) => {
                    let value = match value {
                        serde_json::Value::Null => "---".to_string(),
                        serde_json::Value::Bool(val) => val.to_string(),
                        serde_json::Value::Number(number) => number.to_string(),
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Array(_) => "...".to_string(),
                        serde_json::Value::Object(_) => "...".to_string(),
                    };
                    println!("{}{}:{}{}", indent, key, padding, value)
                }
                Entry::Sublist(sublist) => {
                    println!("{}{}:", indent, key);
                    sublist.print(depth + 1);
                }
            }
        }
        if depth == 0 {
            println!();
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
