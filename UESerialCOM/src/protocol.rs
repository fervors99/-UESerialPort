use crate::types::ProtocolConfig;
use crate::error::{ErrorCode, Result};

/// 协议处理器
pub struct ProtocolHandler {
    config: ProtocolConfig,
}

impl ProtocolHandler {
    pub fn new(config: ProtocolConfig) -> Result<Self> {
        if !config.validate() {
            return Err(ErrorCode::ProtocolError);
        }
        Ok(Self { config })
    }

    pub fn config(&self) -> &ProtocolConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: ProtocolConfig) -> Result<()> {
        if !config.validate() {
            return Err(ErrorCode::ProtocolError);
        }
        self.config = config;
        Ok(())
    }

    /// 在缓冲区中查找完整的数据包
    pub fn find_packet(&self, buffer: &[u8]) -> Option<(usize, usize)> {
        if buffer.len() < self.config.min_length as usize {
            return None;
        }

        // 查找协议头
        let start = buffer.iter().position(|&b| b == self.config.header)?;

        // 确保有足够的数据
        if buffer.len() - start < self.config.min_length as usize {
            return None;
        }

        // 查找协议尾
        let search_range = &buffer[start + 1..];
        let max_search = (self.config.max_length as usize).min(search_range.len());

        for i in (self.config.min_length as usize - 2)..max_search {
            if search_range[i] == self.config.footer {
                let end = start + 1 + i + 1;

                // 验证校验和（如果启用）
                if self.config.use_checksum {
                    if !self.verify_checksum(&buffer[start..end]) {
                        continue;
                    }
                }

                return Some((start, end));
            }
        }

        None
    }

    /// 验证校验和（简单的异或校验）
    fn verify_checksum(&self, packet: &[u8]) -> bool {
        if packet.len() < 3 {
            return false;
        }

        let data = &packet[1..packet.len() - 2];
        let checksum = packet[packet.len() - 2];

        let calculated = data.iter().fold(0u8, |acc, &b| acc ^ b);
        calculated == checksum
    }

    /// 计算校验和
    pub fn calculate_checksum(&self, data: &[u8]) -> u8 {
        data.iter().fold(0u8, |acc, &b| acc ^ b)
    }

    /// 封装数据包
    pub fn wrap_packet(&self, data: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(data.len() + 3);
        packet.push(self.config.header);
        packet.extend_from_slice(data);

        if self.config.use_checksum {
            let checksum = self.calculate_checksum(data);
            packet.push(checksum);
        }

        packet.push(self.config.footer);
        packet
    }

    /// 解包数据（移除协议头尾和校验和）
    pub fn unwrap_packet(&self, packet: &[u8]) -> Result<Vec<u8>> {
        if packet.len() < self.config.min_length as usize {
            return Err(ErrorCode::ProtocolError);
        }

        if packet[0] != self.config.header || packet[packet.len() - 1] != self.config.footer {
            return Err(ErrorCode::ProtocolError);
        }

        let data_end = if self.config.use_checksum {
            packet.len() - 2
        } else {
            packet.len() - 1
        };

        Ok(packet[1..data_end].to_vec())
    }

    /// 处理转义字节
    pub fn escape_data(&self, data: &[u8]) -> Vec<u8> {
        if self.config.escape_byte == 0 {
            return data.to_vec();
        }

        let mut escaped = Vec::with_capacity(data.len());
        for &byte in data {
            if byte == self.config.header
                || byte == self.config.footer
                || byte == self.config.escape_byte
            {
                escaped.push(self.config.escape_byte);
            }
            escaped.push(byte);
        }
        escaped
    }

    /// 反转义数据
    pub fn unescape_data(&self, data: &[u8]) -> Vec<u8> {
        if self.config.escape_byte == 0 {
            return data.to_vec();
        }

        let mut unescaped = Vec::with_capacity(data.len());
        let mut i = 0;

        while i < data.len() {
            if data[i] == self.config.escape_byte && i + 1 < data.len() {
                unescaped.push(data[i + 1]);
                i += 2;
            } else {
                unescaped.push(data[i]);
                i += 1;
            }
        }

        unescaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_handler_creation() {
        let config = ProtocolConfig::default();
        let handler = ProtocolHandler::new(config);
        assert!(handler.is_ok());

        let invalid_config = ProtocolConfig {
            min_length: 1,
            ..Default::default()
        };
        let handler = ProtocolHandler::new(invalid_config);
        assert!(handler.is_err());
    }

    #[test]
    fn test_find_packet() {
        let config = ProtocolConfig::default();
        let handler = ProtocolHandler::new(config).unwrap();

        let buffer = vec![0x00, 0xAA, 0x01, 0x02, 0x55, 0xFF];
        let result = handler.find_packet(&buffer);
        assert_eq!(result, Some((1, 5)));
    }

    #[test]
    fn test_wrap_unwrap_packet() {
        let config = ProtocolConfig::default();
        let handler = ProtocolHandler::new(config).unwrap();

        let data = vec![0x01, 0x02, 0x03];
        let packet = handler.wrap_packet(&data);

        assert_eq!(packet[0], 0xAA);
        assert_eq!(packet[packet.len() - 1], 0x55);

        let unwrapped = handler.unwrap_packet(&packet).unwrap();
        assert_eq!(unwrapped, data);
    }

    #[test]
    fn test_checksum() {
        let config = ProtocolConfig {
            use_checksum: true,
            ..Default::default()
        };
        let handler = ProtocolHandler::new(config).unwrap();

        let data = vec![0x01, 0x02, 0x03];
        let checksum = handler.calculate_checksum(&data);
        assert_eq!(checksum, 0x01 ^ 0x02 ^ 0x03);
    }

    #[test]
    fn test_escape_unescape() {
        let config = ProtocolConfig {
            escape_byte: 0x7D,
            ..Default::default()
        };
        let handler = ProtocolHandler::new(config).unwrap();

        let data = vec![0x01, 0xAA, 0x02, 0x55, 0x03];
        let escaped = handler.escape_data(&data);
        let unescaped = handler.unescape_data(&escaped);

        assert_eq!(unescaped, data);
        assert!(escaped.len() > data.len());
    }
}
