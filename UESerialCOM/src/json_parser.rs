use crate::types::{SensorPoint, SensorData};
use crate::error::{ErrorCode, Result};

/// 快速 JSON 解析器（针对传感器数据优化）
pub struct JsonParser {
    expected_points: u16,
}

impl JsonParser {
    pub fn new(expected_points: u16) -> Self {
        Self { expected_points }
    }

    /// 解析 JSON 格式的传感器数据
    /// 格式: {"Sx1":"0","Sy1":"0","Sc1":"0",...}
    pub fn parse(&self, json_str: &str) -> Result<SensorData> {
        // 快速验证 JSON 格式
        if !json_str.starts_with('{') || !json_str.ends_with('}') {
            return Err(ErrorCode::JsonParseFailed);
        }

        let mut points = Vec::new();
        let mut i = 1;

        // 使用快速字符串查找而不是完整 JSON 解析
        loop {
            let x_key = format!("\"Sx{}\":", i);
            let y_key = format!("\"Sy{}\":", i);
            let c_key = format!("\"Sc{}\":", i);

            match (
                self.extract_value(json_str, &x_key),
                self.extract_value(json_str, &y_key),
                self.extract_value(json_str, &c_key),
            ) {
                (Some(x), Some(y), Some(c)) => {
                    let point = SensorPoint::new(
                        x.parse().unwrap_or(0.0),
                        y.parse().unwrap_or(0.0),
                        c.parse().unwrap_or(0.0),
                    );

                    if !point.is_valid() {
                        eprintln!("[JSON] Warning: Invalid point at index {}", i);
                    }

                    points.push(point);
                    i += 1;
                }
                _ => break,
            }

            // 安全限制：防止无限循环
            if i > 1000 {
                eprintln!("[JSON] Warning: Exceeded max points limit (1000)");
                break;
            }
        }

        // 验证点数量
        if self.expected_points > 0 && points.len() != self.expected_points as usize {
            eprintln!(
                "[JSON] Expected {} points, but found {}",
                self.expected_points,
                points.len()
            );
            return Err(ErrorCode::JsonParseFailed);
        }

        if points.is_empty() {
            return Err(ErrorCode::JsonParseFailed);
        }

        Ok(SensorData::new(points))
    }

    /// 快速提取 JSON 值（避免完整解析）
    fn extract_value<'a>(&self, json: &'a str, key: &str) -> Option<&'a str> {
        let start = json.find(key)? + key.len();
        let remaining = &json[start..];

        // 跳过空格
        let remaining = remaining.trim_start();

        // 提取引号内的值
        if remaining.starts_with('"') {
            let end = remaining[1..].find('"')? + 1;
            Some(&remaining[1..end])
        } else {
            // 提取数字值（直到逗号或右括号）
            let end = remaining
                .find(|c| c == ',' || c == '}')
                .unwrap_or(remaining.len());
            Some(remaining[..end].trim())
        }
    }

    /// 使用 serde_json 作为后备解析器（更健壮但较慢）
    pub fn parse_with_serde(&self, json_str: &str) -> Result<SensorData> {
        let value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|_| ErrorCode::JsonParseFailed)?;

        let mut points = Vec::new();
        let mut i = 1;

        loop {
            let x_key = format!("Sx{}", i);
            let y_key = format!("Sy{}", i);
            let c_key = format!("Sc{}", i);

            match (
                value.get(&x_key).and_then(|v| v.as_str()),
                value.get(&y_key).and_then(|v| v.as_str()),
                value.get(&c_key).and_then(|v| v.as_str()),
            ) {
                (Some(x), Some(y), Some(c)) => {
                    points.push(SensorPoint::new(
                        x.parse().unwrap_or(0.0),
                        y.parse().unwrap_or(0.0),
                        c.parse().unwrap_or(0.0),
                    ));
                    i += 1;
                }
                _ => break,
            }

            if i > 1000 {
                break;
            }
        }

        if self.expected_points > 0 && points.len() != self.expected_points as usize {
            return Err(ErrorCode::JsonParseFailed);
        }

        if points.is_empty() {
            return Err(ErrorCode::JsonParseFailed);
        }

        Ok(SensorData::new(points))
    }
}

/// 在缓冲区中查找完整的 JSON 对象
pub fn find_json_object(buffer: &[u8]) -> Option<(usize, usize)> {
    let buffer_str = std::str::from_utf8(buffer).ok()?;

    let start = buffer_str.find('{')?;
    let end = buffer_str[start..].find('}')?;

    Some((start, start + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parser_basic() {
        let parser = JsonParser::new(2);
        let json = r#"{"Sx1":"1.0","Sy1":"2.0","Sc1":"3.0","Sx2":"4.0","Sy2":"5.0","Sc2":"6.0"}"#;

        let result = parser.parse(json);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.count, 2);
    }

    #[test]
    fn test_json_parser_invalid() {
        let parser = JsonParser::new(0);
        let json = r#"{"invalid": "data"}"#;

        let result = parser.parse(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_parser_point_count_mismatch() {
        let parser = JsonParser::new(5);
        let json = r#"{"Sx1":"1.0","Sy1":"2.0","Sc1":"3.0"}"#;

        let result = parser.parse(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_json_object() {
        let buffer = b"garbage{\"key\":\"value\"}more";
        let result = find_json_object(buffer);

        assert_eq!(result, Some((7, 23)));
    }

    #[test]
    fn test_extract_value() {
        let parser = JsonParser::new(0);
        let json = r#"{"Sx1":"123.45","Sy1":"678.90"}"#;

        let value = parser.extract_value(json, "\"Sx1\":");
        assert_eq!(value, Some("123.45"));
    }
}
