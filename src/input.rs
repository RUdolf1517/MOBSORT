use std::{fmt, fs, path::Path};

use rand::Rng;

use crate::config::MAX_ELEMENTS;

#[derive(Debug)]
pub struct InputError(String);

impl InputError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InputError {}

pub fn random_values(count: usize, min: i64, max: i64) -> Result<Vec<i64>, InputError> {
    if count > MAX_ELEMENTS {
        return Err(InputError::new(format!(
            "Слишком много элементов: максимум {MAX_ELEMENTS}"
        )));
    }
    if min > max {
        return Err(InputError::new("Минимум не может быть больше максимума"));
    }

    let mut rng = rand::thread_rng();
    Ok((0..count).map(|_| rng.gen_range(min..=max)).collect())
}

/// Parses signed integers separated by whitespace, commas, or semicolons.
/// Lines may contain comments after `#`, which is useful for TXT files.
pub fn parse_values(text: &str) -> Result<Vec<i64>, InputError> {
    let mut values = Vec::new();

    for line in text.lines() {
        let content = line.split_once('#').map_or(line, |(before, _)| before);
        for token in content.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            let token = token.trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | '"' | '\''));
            if token.is_empty() {
                continue;
            }
            let value = token.parse::<i64>().map_err(|_| {
                InputError::new(format!("Некорректное целое число: {token:?}"))
            })?;
            values.push(value);
            if values.len() > MAX_ELEMENTS {
                return Err(InputError::new(format!(
                    "Слишком много элементов: максимум {MAX_ELEMENTS}"
                )));
            }
        }
    }

    Ok(values)
}

pub fn load_file(path: impl AsRef<Path>) -> Result<Vec<i64>, InputError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| {
        InputError::new(format!("Не удалось прочитать {}: {error}", path.display()))
    })?;
    parse_values(&text)
}

#[cfg(test)]
mod tests {
    use super::{parse_values, random_values};

    #[test]
    fn parses_signed_values_and_ignores_comments() {
        assert_eq!(
            parse_values("-8, 0; 14\n3 # trailing note\n").unwrap(),
            vec![-8, 0, 14, 3]
        );
        assert!(parse_values(" \n # empty\n").unwrap().is_empty());
        assert!(parse_values("1, nope, 3").is_err());
    }

    #[test]
    fn random_source_honors_bounds_and_rejects_bad_ranges() {
        let values = random_values(128, -5, 7).unwrap();
        assert_eq!(values.len(), 128);
        assert!(values.iter().all(|value| (-5..=7).contains(value)));
        assert!(random_values(1, 5, -5).is_err());
    }
}
