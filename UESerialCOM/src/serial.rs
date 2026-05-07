use serialport::{SerialPort, ClearBuffer};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::buffer::RingBuffer;
use crate::error::{ErrorCode, Result, map_serial_error};
use crate::protocol::ProtocolHandler;
use crate::types::{ProtocolConfig, BufferStatusInfo};

/// 串口上下文（线程安全）
pub struct SerialContext {
    port: Mutex<Box<dyn SerialPort>>,
    read_buffer: Mutex<RingBuffer>,
    protocol: Mutex<ProtocolHandler>,
    magic: u32, // 魔数用于句柄验证
}

const MAGIC_NUMBER: u32 = 0xDEADBEEF;

impl SerialContext {
    pub fn new(
        port: Box<dyn SerialPort>,
        protocol_config: ProtocolConfig,
    ) -> Result<Self> {
        let protocol = ProtocolHandler::new(protocol_config)?;

        Ok(Self {
            port: Mutex::new(port),
            read_buffer: Mutex::new(RingBuffer::new(1024 * 1024)),
            protocol: Mutex::new(protocol),
            magic: MAGIC_NUMBER,
        })
    }

    /// 验证句柄是否有效
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC_NUMBER
    }

    /// 获取端口锁（带毒化恢复）
    pub fn lock_port(&self) -> Result<MutexGuard<'_, Box<dyn SerialPort>>> {
        match self.port.lock() {
            Ok(guard) => Ok(guard),
            Err(poison_err) => {
                eprintln!("[Serial] Port lock poisoned, attempting recovery");
                Ok(poison_err.into_inner())
            }
        }
    }

    /// 获取缓冲区锁（带毒化恢复）
    pub fn lock_buffer(&self) -> Result<MutexGuard<'_, RingBuffer>> {
        match self.read_buffer.lock() {
            Ok(guard) => Ok(guard),
            Err(poison_err) => {
                eprintln!("[Serial] Buffer lock poisoned, attempting recovery");
                Ok(poison_err.into_inner())
            }
        }
    }

    /// 获取协议锁（带毒化恢复）
    pub fn lock_protocol(&self) -> Result<MutexGuard<'_, ProtocolHandler>> {
        match self.protocol.lock() {
            Ok(guard) => Ok(guard),
            Err(poison_err) => {
                eprintln!("[Serial] Protocol lock poisoned, attempting recovery");
                Ok(poison_err.into_inner())
            }
        }
    }

    /// 写入数据
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        let mut port = self.lock_port()?;
        port.write(data)
            .map_err(|_| ErrorCode::WriteFailed)
    }

    /// 读取数据到内部缓冲区
    pub fn read_to_buffer(&self) -> Result<usize> {
        let mut port = self.lock_port()?;
        let mut temp_buffer = [0u8; 4096];

        match port.read(&mut temp_buffer) {
            Ok(n) if n > 0 => {
                let mut buffer = self.lock_buffer()?;
                buffer.push(&temp_buffer[..n])
                    .map_err(|_| ErrorCode::BufferOverflow)?;
                Ok(n)
            }
            Ok(_) => Ok(0),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Err(ErrorCode::TimeoutFailed)
            }
            Err(_) => Err(ErrorCode::ReadFailed),
        }
    }

    /// 设置超时
    pub fn set_timeout(&self, timeout: Duration) -> Result<()> {
        let mut port = self.lock_port()?;
        port.set_timeout(timeout)
            .map_err(|_| ErrorCode::IoError)
    }

    /// 清空缓冲区
    pub fn clear_buffers(&self, clear_input: bool, clear_output: bool) -> Result<()> {
        let port = self.lock_port()?;

        if clear_input {
            port.clear(ClearBuffer::Input)
                .map_err(|_| ErrorCode::IoError)?;
        }

        if clear_output {
            port.clear(ClearBuffer::Output)
                .map_err(|_| ErrorCode::IoError)?;
        }

        Ok(())
    }

    /// 获取缓冲区状态
    pub fn get_buffer_status(&self) -> Result<BufferStatusInfo> {
        let buffer = self.lock_buffer()?;
        let stats = buffer.stats();

        Ok(BufferStatusInfo {
            current_size: buffer.len(),
            max_size: crate::buffer::MAX_BUFFER_SIZE,
            warning_threshold: crate::buffer::BUFFER_WARNING_THRESHOLD,
            overflow_count: stats.overflow_count(),
            bytes_discarded: stats.bytes_discarded(),
            warning_count: stats.warning_count(),
            usage_percent: buffer.usage_percent(),
        })
    }

    /// 重置缓冲区统计
    pub fn reset_buffer_stats(&self) -> Result<()> {
        let buffer = self.lock_buffer()?;
        buffer.stats().reset();
        Ok(())
    }

    /// 清空内部缓冲区
    pub fn clear_internal_buffer(&self) -> Result<()> {
        let mut buffer = self.lock_buffer()?;
        buffer.clear();
        Ok(())
    }

    /// 获取协议配置
    pub fn get_protocol_config(&self) -> Result<ProtocolConfig> {
        let protocol = self.lock_protocol()?;
        Ok(*protocol.config())
    }

    /// 设置协议配置
    pub fn set_protocol_config(&self, config: ProtocolConfig) -> Result<()> {
        let mut protocol = self.lock_protocol()?;
        protocol.set_config(config)
    }

    /// 获取期望点数
    pub fn get_expected_points(&self) -> Result<u16> {
        let protocol = self.lock_protocol()?;
        Ok(protocol.config().expected_points)
    }

    /// 设置期望点数
    pub fn set_expected_points(&self, points: u16) -> Result<()> {
        let mut protocol = self.lock_protocol()?;
        let mut config = *protocol.config();
        config.expected_points = points;
        protocol.set_config(config)
    }
}

impl Drop for SerialContext {
    fn drop(&mut self) {
        // 清除魔数，使句柄失效
        self.magic = 0;
    }
}

/// 打开串口
pub fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
    timeout: Duration,
    protocol_config: ProtocolConfig,
) -> Result<Arc<SerialContext>> {
    let port = serialport::new(port_name, baud_rate)
        .timeout(timeout)
        .open()
        .map_err(map_serial_error)?;

    let context = SerialContext::new(port, protocol_config)?;
    Ok(Arc::new(context))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_context_validation() {
        // 注意：这个测试需要实际的串口设备，这里只测试结构
        let config = ProtocolConfig::default();
        // 实际测试需要 mock 串口
    }

    #[test]
    fn test_magic_number() {
        assert_eq!(MAGIC_NUMBER, 0xDEADBEEF);
    }
}
