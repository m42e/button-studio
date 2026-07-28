use csv::{ReaderBuilder, Trim};
use std::{collections::HashMap, fs, path::Path};

#[derive(Clone, Debug)]
pub struct SeriesData {
    pub source_name: String,
    pub columns: Vec<String>,
    pub rows: Vec<HashMap<String, String>>,
}

impl SeriesData {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("Could not read CSV: {error}"))?;
        let source_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("series.csv")
            .to_owned();
        Self::from_bytes(&bytes, source_name)
    }

    fn from_bytes(bytes: &[u8], source_name: String) -> Result<Self, String> {
        let delimiter = detect_delimiter(bytes);
        let mut reader = ReaderBuilder::new()
            .delimiter(delimiter)
            .trim(Trim::All)
            .from_reader(bytes);
        let mut columns = reader
            .headers()
            .map_err(|error| format!("Could not read CSV headers: {error}"))?
            .iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if let Some(first) = columns.first_mut() {
            *first = first.trim_start_matches('\u{feff}').to_owned();
        }
        if columns.is_empty() || columns.iter().any(|column| column.is_empty()) {
            return Err("Every CSV column needs a non-empty header".to_owned());
        }
        let mut unique = std::collections::HashSet::new();
        if let Some(duplicate) = columns
            .iter()
            .find(|column| !unique.insert(column.as_str()))
        {
            return Err(format!("CSV column '{duplicate}' appears more than once"));
        }

        let mut rows = Vec::new();
        for (index, record) in reader.records().enumerate() {
            let record =
                record.map_err(|error| format!("Could not read CSV row {}: {error}", index + 2))?;
            rows.push(
                columns
                    .iter()
                    .cloned()
                    .zip(record.iter().map(str::to_owned))
                    .collect(),
            );
        }
        if rows.is_empty() {
            return Err("CSV contains headers but no entries".to_owned());
        }

        Ok(Self {
            source_name,
            columns,
            rows,
        })
    }
}

fn detect_delimiter(bytes: &[u8]) -> u8 {
    let mut counts = [(b',', 0), (b';', 0), (b'\t', 0)];
    let mut quoted = false;
    for byte in bytes.iter().copied() {
        match byte {
            b'"' => quoted = !quoted,
            b'\n' | b'\r' if !quoted => break,
            _ if !quoted => {
                if let Some((_, count)) =
                    counts.iter_mut().find(|(candidate, _)| *candidate == byte)
                {
                    *count += 1;
                }
            }
            _ => {}
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| *count > 0)
        .map(|(delimiter, _)| delimiter)
        .unwrap_or(b',')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_import_detects_semicolons_and_maps_columns() {
        let data = SeriesData::from_bytes(
            b"name;team;url\nAda;Research;https://example.com/42\nLinus;Kernel;https://example.com/7\n",
            "people.csv".to_owned(),
        )
        .expect("parse CSV");

        assert_eq!(data.columns, ["name", "team", "url"]);
        assert_eq!(data.rows.len(), 2);
        assert_eq!(data.rows[0]["name"], "Ada");
        assert_eq!(data.rows[1]["url"], "https://example.com/7");
    }

    #[test]
    fn csv_import_rejects_duplicate_headers() {
        let error = SeriesData::from_bytes(b"name,name\nAda,Lovelace\n", "people.csv".to_owned())
            .expect_err("reject duplicate headers");

        assert!(error.contains("appears more than once"));
    }
}
