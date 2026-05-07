use serialport::{SerialPort, ClearBuffer}; // 确保这里导入了 ClearBuffer
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;
use std::time::Duration;

// ==========================================
// 1. 常量与结构体定义
// ==========================================

// 细化的错误码
const ERROR_OK: c_int = 0;
const ERROR_INVALID_ARGS: c_int = 1;
const ERROR_OPEN_FAILED: c_int = 2;
const ERROR_NOT_FOUND: c_int = 3;           // 设备不存在
const ERROR_WRITE_FAILED: c_int = 4;
const ERROR_READ_FAILED: c_int = 5;
const ERROR_TIMEOUT_FAILED: c_int = 6;
const ERROR_PERMISSION_DENIED: c_int = 7;   // 权限不足
const ERROR_IO_ERROR: c_int = 8;            // 通用IO错误
const ERROR_JSON_PARSE_FAILED: c_int = 9;   // JSON解析失败

// 缓冲区管理常量
const MAX_BUFFER_SIZE: usize = 1024 * 1024;              // 1MB 最大缓冲区
const BUFFER_TRIM_SIZE: usize = 512 * 1024;              // 保留 512KB
const BUFFER_WARNING_THRESHOLD: usize = MAX_BUFFER_SIZE * 80 / 100; // 80% 预警

/// 传感器点数据
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SensorPoint {
    pub x: f32,
    pub y: f32,
    pub c: f32,
}

/// 传感器数据（动态点数）
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SensorData {
    pub points: *mut SensorPoint,
    pub count: usize,
    pub capacity: usize,
}

impl Default for SensorData {
    fn default() -> Self {
        SensorData {
            points: std::ptr::null_mut(),
            count: 0,
            capacity: 0,
        }
    }
}

impl Drop for SensorData {
    fn drop(&mut self) {
        if !self.points.is_null() && self.capacity > 0 {
            unsafe {
                let _ = Vec::from_raw_parts(self.points, self.count, self.capacity);
            }
        }
    }
}

/// 协议配置
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ProtocolConfig {
    pub header: u8,         // 协议头字节
    pub footer: u8,         // 协议尾字节
    pub min_length: u8,     // 最小数据包长度（包含头尾）
    pub max_length: u16,    // 最大数据包长度（包含头尾）
    pub use_checksum: bool, // 是否启用校验和（倒数第二字节）
    pub escape_byte: u8,    // 转义字节（0表示不使用转义）
    pub expected_points: u16, // 期望的传感器点数量（0表示不检查）
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        ProtocolConfig {
            header: 0xAA,
            footer: 0x55,
            min_length: 4,
            max_length: 256,
            use_checksum: false,
            escape_byte: 0,
            expected_points: 21, // 默认期望21个点
        }
    }
}

impl ProtocolConfig {
    /// 验证协议配置的有效性
    fn validate(&self) -> bool {
        // 最小长度至少为2（头+尾）
        if self.min_length < 2 {
            return false;
        }
        // 最大长度必须大于等于最小长度
        if self.max_length < self.min_length as u16 {
            return false;
        }
        // 如果启用校验和，最小长度至少为3（头+校验+尾）
        if self.use_checksum && self.min_length < 3 {
            return false;
        }
        // 头尾不能相同（除非都是0，表示不使用）
        if self.header == self.footer && self.header != 0 {
            return false;
        }
        true
    }
}

/// 缓冲区统计信息
#[derive(Default)]
struct BufferStats {
    overflow_count: std::sync::atomic::AtomicUsize,     // 溢出次数
    bytes_discarded: std::sync::atomic::AtomicUsize,    // 丢弃的字节数
    warning_count: std::sync::atomic::AtomicUsize,      // 警告次数
}

/// 串口上下文
/// 包含串口句柄、内部缓冲区和协议配置
struct SerialContext {
    port: Mutex<Box<dyn SerialPort>>,
    read_buffer: Mutex<Vec<u8>>,
    protocol: ProtocolConfig,  // 协议配置
    buffer_stats: BufferStats, // 缓冲区统计
}

// ==========================================
// 2. 辅助函数与宏
// ==========================================

// 细化的错误类型映射
fn map_serial_error(err: serialport::Error) -> c_int {
    match err.kind() {
        serialport::ErrorKind::NoDevice => ERROR_NOT_FOUND,
        serialport::ErrorKind::Io(io_kind) => match io_kind {
            std::io::ErrorKind::TimedOut => ERROR_TIMEOUT_FAILED,
            std::io::ErrorKind::PermissionDenied => ERROR_PERMISSION_DENIED,
            _ => ERROR_IO_ERROR,
        },
        _ => ERROR_OPEN_FAILED,
    }
}

// 锁中毒恢复：从PoisonError中恢复锁的所有权
fn recover_port_lock<'a>(
    context: &'a SerialContext
) -> Result<std::sync::MutexGuard<'a, Box<dyn SerialPort>>, c_int> {
    context.port.lock()
        .or_else(|poison_err| {
            eprintln!("Port lock poisoned, attempting recovery...");
            Ok::<std::sync::MutexGuard<'a, Box<dyn SerialPort>>, std::sync::PoisonError<std::sync::MutexGuard<'a, Box<dyn SerialPort>>>>(poison_err.into_inner())
        })
        .map_err(|_| ERROR_INVALID_ARGS)
}

fn recover_buffer_lock<'a>(
    context: &'a SerialContext
) -> Result<std::sync::MutexGuard<'a, Vec<u8>>, c_int> {
    context.read_buffer.lock()
        .or_else(|poison_err| {
            eprintln!("Buffer lock poisoned, attempting recovery...");
            Ok::<std::sync::MutexGuard<'a, Vec<u8>>, std::sync::PoisonError<std::sync::MutexGuard<'a, Vec<u8>>>>(poison_err.into_inner())
        })
        .map_err(|_| ERROR_INVALID_ARGS)
}

// C 字符串转 Rust 字符串
fn c_string_to_rust(port_name: *const c_char) -> Result<String, ()> {
    if port_name.is_null() { return Err(()); }
    unsafe {
        CStr::from_ptr(port_name)
            .to_str()
            .map(|s| s.to_owned())
            .map_err(|_| ())
    }
}

/// 解析JSON格式的传感器数据
/// 输入格式: {"Sx1":"0","Sy1":"0","Sc1":"0",...,"SxN":"0","SyN":"0","ScN":"0"}
/// expected_points: 期望的点数量，0表示不检查
fn parse_sensor_json(json_str: &str, expected_points: u16) -> Result<SensorData, String> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut points = Vec::new();
    let mut i = 1;

    // 动态解析所有可用的点
    loop {
        let x_key = format!("Sx{}", i);
        let y_key = format!("Sy{}", i);
        let c_key = format!("Sc{}", i);

        match (
            value.get(&x_key).and_then(|v| v.as_str()),
            value.get(&y_key).and_then(|v| v.as_str()),
            value.get(&c_key).and_then(|v| v.as_str()),
        ) {
            (Some(x_val), Some(y_val), Some(c_val)) => {
                points.push(SensorPoint {
                    x: x_val.parse().unwrap_or(0.0),
                    y: y_val.parse().unwrap_or(0.0),
                    c: c_val.parse().unwrap_or(0.0),
                });
                i += 1;
            }
            _ => break, // 没有更多点了
        }
    }

    // 如果指定了期望点数，进行验证
    if expected_points > 0 && points.len() != expected_points as usize {
        return Err(format!(
            "Expected {} points, but found {}",
            expected_points,
            points.len()
        ));
    }

    // 转换为C兼容的结构
    let count = points.len();
    let capacity = points.capacity();
    let ptr = points.as_mut_ptr();
    std::mem::forget(points); // 防止Vec被drop

    Ok(SensorData {
        points: ptr,
        count,
        capacity,
    })
}

/// Panic 捕获宏：防止 Rust 崩溃穿透到 UE5
macro_rules! catch_unwind {
    ($body:block) => {
        match panic::catch_unwind(AssertUnwindSafe(|| $body)) {
            Ok(result) => result,
            Err(_) => {
                eprintln!("Rust FFI Panic caught!");
                return ERROR_OPEN_FAILED;
            }
        }
    };
}

// ==========================================
// 3. FFI 接口实现 (暴露给 UE5)
// ==========================================

// 打开串口
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_open(
    port_name: *const c_char,
    baud_rate: c_uint,
    timeout_ms: c_uint,
    out_handle: *mut *mut c_void,
) -> c_int {
    catch_unwind!({
        if port_name.is_null() || out_handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let name = match c_string_to_rust(port_name) {
            Ok(s) => s,
            Err(_) => return ERROR_INVALID_ARGS,
        };

        let timeout = Duration::from_millis(timeout_ms as u64);

        match serialport::new(name, baud_rate).timeout(timeout).open() {
            Ok(port) => {
                let context = Box::new(SerialContext {
                    port: Mutex::new(port),
                    read_buffer: Mutex::new(Vec::with_capacity(4096)), // 预分配 4KB 缓冲区
                    protocol: ProtocolConfig::default(), // 使用默认协议配置
                    buffer_stats: BufferStats::default(), // 初始化统计
                });

                //  【关键步骤】在返回句柄前，立即清空硬件驱动层的残留数据
                if let Ok(port_guard) = recover_port_lock(&context) {
                    // 忽略清空失败的错误，通常刚打开时不会失败
                    let _ = port_guard.clear(ClearBuffer::Input);
                }

                unsafe {
                    *out_handle = Box::into_raw(context) as *mut c_void;
                }
                ERROR_OK
            }
            Err(err) => map_serial_error(err),
        }
    })
}

// 打开串口（带自定义协议配置）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_open_with_protocol(
    port_name: *const c_char,
    baud_rate: c_uint,
    timeout_ms: c_uint,
    protocol_config: *const ProtocolConfig,
    out_handle: *mut *mut c_void,
) -> c_int {
    catch_unwind!({
        if port_name.is_null() || out_handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let name = match c_string_to_rust(port_name) {
            Ok(s) => s,
            Err(_) => return ERROR_INVALID_ARGS,
        };

        let timeout = Duration::from_millis(timeout_ms as u64);

        // 获取协议配置（如果为null则使用默认）
        let protocol = if protocol_config.is_null() {
            ProtocolConfig::default()
        } else {
            let config = unsafe { *protocol_config };
            // 验证协议配置
            if !config.validate() {
                return ERROR_INVALID_ARGS;
            }
            config
        };

        match serialport::new(name, baud_rate).timeout(timeout).open() {
            Ok(port) => {
                let context = Box::new(SerialContext {
                    port: Mutex::new(port),
                    read_buffer: Mutex::new(Vec::with_capacity(4096)),
                    protocol, // 使用自定义协议配置
                    buffer_stats: BufferStats::default(), // 初始化统计
                });

                if let Ok(port_guard) = recover_port_lock(&context) {
                    let _ = port_guard.clear(ClearBuffer::Input);
                }

                unsafe {
                    *out_handle = Box::into_raw(context) as *mut c_void;
                }
                ERROR_OK
            }
            Err(err) => map_serial_error(err),
        }
    })
}

//关闭串口
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_close(handle: *mut c_void) {
    if handle.is_null() { return; }
    unsafe {
        let _ = Box::from_raw(handle as *mut SerialContext);
    }
}

//写入数据
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_write(
    handle: *mut c_void,
    data: *const u8,
    length: usize,
    out_written: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || data.is_null() || out_written.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };
        let buffer = unsafe { std::slice::from_raw_parts(data, length) };

        let mut guard = match recover_port_lock(context) {
            Ok(g) => g,
            Err(e) => return e,
        };

        match guard.write(buffer) {
            Ok(written) => {
                unsafe { *out_written = written; }
                ERROR_OK
            }
            Err(_) => ERROR_WRITE_FAILED,
        }
    })
}

//底层读取：从硬件读取数据并追加到内部缓冲区
// 优化：先读取数据到临时栈内存，释放串口锁，然后再加缓冲区锁写入
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_read_raw(
    handle: *mut c_void,
    temp_buf: *mut u8,
    max_len: usize,
    out_read: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || temp_buf.is_null() || out_read.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };
        let buffer_slice = unsafe { std::slice::from_raw_parts_mut(temp_buf, max_len) };

        // 1. 先获取串口锁并读取数据
        let bytes_read = {
            let mut port_guard = match recover_port_lock(context) {
                Ok(g) => g,
                Err(e) => return e,
            };

            match port_guard.read(buffer_slice) {
                Ok(n) => n,
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::TimedOut {
                        unsafe { *out_read = 0; }
                        return ERROR_OK;
                    } else {
                        return ERROR_READ_FAILED;
                    }
                }
            }
        }; // 串口锁在这里释放

        // 2. 串口锁已释放，现在获取缓冲区锁并写入
        if bytes_read > 0 {
            match recover_buffer_lock(context) {
                Ok(mut internal_buf) => {
                    internal_buf.extend_from_slice(&buffer_slice[..bytes_read]);

                    // 缓冲区溢出管理
                    let current_size = internal_buf.len();

                    // 80% 预警
                    if current_size > BUFFER_WARNING_THRESHOLD {
                        context.buffer_stats.warning_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        eprintln!(
                            "Warning: Buffer usage high ({}/{} bytes, {:.1}%). Consider increasing read frequency.",
                            current_size,
                            MAX_BUFFER_SIZE,
                            (current_size as f64 / MAX_BUFFER_SIZE as f64) * 100.0
                        );
                    }

                    // 100% 溢出处理
                    if current_size > MAX_BUFFER_SIZE {
                        let keep_from = current_size - BUFFER_TRIM_SIZE;
                        let discarded = keep_from;

                        context.buffer_stats.overflow_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        context.buffer_stats.bytes_discarded.fetch_add(discarded, std::sync::atomic::Ordering::Relaxed);

                        eprintln!(
                            "Critical: Buffer overflow! Discarding {} bytes of old data. (Overflow #{}, Total discarded: {} bytes)",
                            discarded,
                            context.buffer_stats.overflow_count.load(std::sync::atomic::Ordering::Relaxed),
                            context.buffer_stats.bytes_discarded.load(std::sync::atomic::Ordering::Relaxed)
                        );

                        internal_buf.drain(..keep_from);
                    }
                }
                Err(_) => return ERROR_READ_FAILED,
            }
        }

        unsafe { *out_read = bytes_read; }
        ERROR_OK
    })
}

/// 计算简单校验和（所有数据字节异或）
fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc ^ b)
}

/// 构建带协议头尾和校验和的数据包
/// data: 原始数据（不包含头尾和校验和）
/// protocol: 协议配置
/// out_buffer: 输出缓冲区
/// max_len: 输出缓冲区最大长度
/// 返回：实际写入的字节数，如果缓冲区不足返回0
fn build_packet(
    data: &[u8],
    protocol: &ProtocolConfig,
    out_buffer: &mut [u8],
) -> usize {
    let header = protocol.header;
    let footer = protocol.footer;
    let use_checksum = protocol.use_checksum;

    // 计算所需的总长度
    let total_len = if use_checksum {
        1 + data.len() + 1 + 1 // header + data + checksum + footer
    } else {
        1 + data.len() + 1 // header + data + footer
    };

    // 检查缓冲区是否足够
    if total_len > out_buffer.len() {
        return 0;
    }

    let mut pos = 0;

    // 写入协议头
    out_buffer[pos] = header;
    pos += 1;

    // 写入数据
    out_buffer[pos..pos + data.len()].copy_from_slice(data);
    pos += data.len();

    // 写入校验和（如果启用）
    if use_checksum {
        let checksum = calculate_checksum(data);
        out_buffer[pos] = checksum;
        pos += 1;
    }

    // 写入协议尾
    out_buffer[pos] = footer;
    pos += 1;

    pos
}

/// 构建数据包（FFI接口）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_build_packet(
    handle: *mut c_void,
    data: *const u8,
    data_len: usize,
    out_buffer: *mut u8,
    max_len: usize,
    out_len: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || data.is_null() || out_buffer.is_null() || out_len.is_null() {
            return ERROR_INVALID_ARGS;
        }

        if data_len == 0 || max_len == 0 {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };
        let input_data = unsafe { std::slice::from_raw_parts(data, data_len) };
        let output_buffer = unsafe { std::slice::from_raw_parts_mut(out_buffer, max_len) };

        let written = build_packet(input_data, &context.protocol, output_buffer);

        if written == 0 {
            return ERROR_INVALID_ARGS; // 缓冲区不足
        }

        unsafe {
            *out_len = written;
        }

        ERROR_OK
    })
}

/// 提取数据包的有效数据（去除协议头尾和校验和）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_extract_payload(
    handle: *mut c_void,
    packet: *const u8,
    packet_len: usize,
    out_buffer: *mut u8,
    max_len: usize,
    out_len: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || packet.is_null() || out_buffer.is_null() || out_len.is_null() {
            return ERROR_INVALID_ARGS;
        }

        if packet_len == 0 || max_len == 0 {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };
        let packet_data = unsafe { std::slice::from_raw_parts(packet, packet_len) };

        let use_checksum = context.protocol.use_checksum;
        let min_length = context.protocol.min_length as usize;

        // 验证包长度
        if packet_len < min_length {
            return ERROR_INVALID_ARGS;
        }

        // 计算有效数据的起始和结束位置
        let payload_start = 1; // 跳过协议头
        let payload_end = if use_checksum {
            packet_len - 2 // 去除校验和和协议尾
        } else {
            packet_len - 1 // 去除协议尾
        };

        if payload_start >= payload_end {
            // 没有有效数据
            unsafe { *out_len = 0; }
            return ERROR_OK;
        }

        let payload_len = payload_end - payload_start;

        if payload_len > max_len {
            return ERROR_INVALID_ARGS; // 输出缓冲区太小
        }

        // 复制有效数据
        unsafe {
            std::ptr::copy_nonoverlapping(
                packet_data[payload_start..payload_end].as_ptr(),
                out_buffer,
                payload_len
            );
            *out_len = payload_len;
        }

        ERROR_OK
    })
}

/// 解析数据：从内部缓冲区提取完整包（解决粘包）
/// 使用配置的协议参数进行解析
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_parse_data(
    handle: *mut c_void,
    out_buffer: *mut u8,
    max_len: usize,
    out_len: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_buffer.is_null() || out_len.is_null() {
            return ERROR_INVALID_ARGS;
        }

        if max_len == 0 {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        // 获取协议配置
        let header = context.protocol.header;
        let footer = context.protocol.footer;
        let min_length = context.protocol.min_length as usize;
        let max_length = context.protocol.max_length as usize;
        let use_checksum = context.protocol.use_checksum;

        let mut internal_buf = match recover_buffer_lock(context) {
            Ok(buf) => buf,
            Err(_) => return ERROR_READ_FAILED,
        };

        // 如果缓冲区为空，直接返回
        if internal_buf.is_empty() {
            unsafe { *out_len = 0; }
            return ERROR_OK;
        }

        // 循环查找有效数据包（处理连续的无效数据）
        loop {
            // 查找协议头
            let start_pos = match internal_buf.iter().position(|&b| b == header) {
                Some(pos) => pos,
                None => {
                    // 没有找到协议头，清空所有数据
                    internal_buf.clear();
                    unsafe { *out_len = 0; }
                    return ERROR_OK;
                }
            };

            // 如果协议头不在开头，丢弃之前的垃圾数据
            if start_pos > 0 {
                internal_buf.drain(..start_pos);
            }

            // 检查是否有足够的数据来查找协议尾
            if internal_buf.len() < min_length {
                // 数据不足，等待更多数据
                unsafe { *out_len = 0; }
                return ERROR_OK;
            }

            // 从协议头之后开始查找协议尾（跳过第一个字节，避免头尾相同时的问题）
            let footer_search_start = 1;

            let end_pos = match internal_buf[footer_search_start..].iter().position(|&b| b == footer) {
                Some(pos) => footer_search_start + pos,
                None => {
                    // 没有找到协议尾
                    // 如果缓冲区已经超过最大长度，说明这个包无效
                    if internal_buf.len() > max_length {
                        // 丢弃当前协议头，继续查找下一个
                        internal_buf.drain(..1);
                        continue;
                    }
                    // 否则等待更多数据
                    unsafe { *out_len = 0; }
                    return ERROR_OK;
                }
            };

            let real_end = end_pos + 1; // 包含协议尾
            let packet = &internal_buf[0..real_end];

            // 验证包长度
            if packet.len() < min_length {
                // 包太小，丢弃到协议尾，继续查找
                internal_buf.drain(..real_end);
                continue;
            }

            if packet.len() > max_length {
                // 包太大，丢弃当前协议头，继续查找
                internal_buf.drain(..1);
                continue;
            }

            // 校验和验证（如果启用）
            if use_checksum {
                if packet.len() < 3 {
                    // 长度不足以包含校验和
                    internal_buf.drain(..real_end);
                    continue;
                }

                // 校验和在倒数第二个字节（footer前一个字节）
                let checksum_pos = packet.len() - 2;
                let received_checksum = packet[checksum_pos];

                // 计算数据部分的校验和（不包括header、checksum和footer）
                let data_for_checksum = &packet[1..checksum_pos];
                let calculated_checksum = calculate_checksum(data_for_checksum);

                if received_checksum != calculated_checksum {
                    // 校验和错误，丢弃这个包
                    internal_buf.drain(..real_end);
                    continue;
                }
            }

            // 检查输出缓冲区是否足够大
            if packet.len() > max_len {
                return ERROR_INVALID_ARGS; // 输出缓冲区太小
            }

            // 复制有效数据包到输出缓冲区
            unsafe {
                std::ptr::copy_nonoverlapping(packet.as_ptr(), out_buffer, packet.len());
                *out_len = packet.len();
            }

            // 从缓冲区移除已处理的数据
            internal_buf.drain(..real_end);
            return ERROR_OK;
        }
    })
}

/// 获取当前协议配置
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_get_protocol_config(
    handle: *mut c_void,
    out_config: *mut ProtocolConfig,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_config.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        unsafe {
            *out_config = context.protocol;
        }

        ERROR_OK
    })
}

/// 设置协议配置（运行时动态修改）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_set_protocol_config(
    handle: *mut c_void,
    new_config: *const ProtocolConfig,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || new_config.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let config = unsafe { *new_config };

        // 验证配置有效性
        if !config.validate() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &mut *(handle as *mut SerialContext) };
        context.protocol = config;

        ERROR_OK
    })
}

/// 获取内部缓冲区中的数据量
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_get_buffer_size(
    handle: *mut c_void,
    out_size: *mut usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_size.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_buffer_lock(context) {
            Ok(buf) => {
                unsafe { *out_size = buf.len(); }
                ERROR_OK
            }
            Err(e) => e,
        }
    })
}

//清理缓冲区：同时清空硬件驱动层和软件内部层的缓存
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_flush_input(handle: *mut c_void) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                // 1. 清空硬件/驱动层的缓冲区
                match port_guard.clear(ClearBuffer::Input) {
                    Ok(()) => {
                        // 2. 清空 Rust 内部软件层的缓冲区
                        if let Ok(mut buf) = recover_buffer_lock(context) {
                            buf.clear();
                        }
                        ERROR_OK
                    },
                    Err(_) => ERROR_READ_FAILED,
                }
            }
            Err(e) => e,
        }
    })
}

// --- 新增的手动清空缓冲区功能 ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_clear_buffers(
    handle: *mut c_void,
    clear_input: bool,
    clear_output: bool,
) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        if !clear_input && !clear_output {
            return ERROR_INVALID_ARGS; // 两个都不清空没有意义
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                let clear_flags = if clear_input && clear_output {
                    ClearBuffer::All
                } else if clear_input {
                    ClearBuffer::Input
                } else {
                    ClearBuffer::Output
                };

                match port_guard.clear(clear_flags) {
                    Ok(()) => {
                        // 释放串口锁
                        drop(port_guard);

                        // 如果清空输入缓冲区，也清空软件缓冲区
                        if clear_input
                            && let Ok(mut buf) = recover_buffer_lock(context) {
                                buf.clear();
                            }
                        ERROR_OK
                    }
                    Err(_) => ERROR_READ_FAILED,
                }
            }
            Err(e) => e,
        }
    })
}



// --- 串口健康检查功能 ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_is_alive(handle: *mut c_void) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return 0; // 无效句柄
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                // 尝试获取串口名称，如果失败说明串口已断开
                match port_guard.name() {
                    Some(_) => 1, // 串口存活
                    None => 0,    // 串口已断开
                }
            }
            Err(_) => 0, // 锁恢复失败，视为不可用
        }
    })
}

// --- 串口重启功能（关闭并重新打开）---
// 原子操作：先尝试打开新连接，成功后才替换旧连接
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_restart(
    handle: *mut c_void,
    port_name: *const c_char,
    baud_rate: c_uint,
    timeout_ms: c_uint,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || port_name.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let name = match c_string_to_rust(port_name) {
            Ok(s) => s,
            Err(_) => return ERROR_INVALID_ARGS,
        };

        let context = unsafe { &*(handle as *mut SerialContext) };
        let timeout = Duration::from_millis(timeout_ms as u64);

        // 1. 先尝试打开新的串口连接（在锁外进行，避免阻塞）
        let new_port = match serialport::new(name, baud_rate).timeout(timeout).open() {
            Ok(port) => port,
            Err(err) => return map_serial_error(err), // 打开失败，保持旧连接不变
        };

        // 2. 打开成功后，获取锁并替换旧连接
        match recover_port_lock(context) {
            Ok(mut port_guard) => {
                // 3. 替换旧的串口句柄（旧连接会自动drop关闭）
                *port_guard = new_port;

                // 4. 清空硬件缓冲区（重要：从干净状态开始）
                let _ = port_guard.clear(ClearBuffer::All);

                // 释放串口锁
                drop(port_guard);

                // 5. 清空软件缓冲区
                if let Ok(mut buf) = recover_buffer_lock(context) {
                    buf.clear();
                }

                ERROR_OK
            }
            Err(_) => {
                // 锁恢复失败，但新连接已打开，这里会自动drop新连接
                ERROR_INVALID_ARGS
            }
        }
    })
}

// --- 获取串口名称 ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_get_port_name(
    handle: *mut c_void,
    out_buffer: *mut c_char,
    buffer_size: usize,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_buffer.is_null() || buffer_size == 0 {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                match port_guard.name() {
                    Some(name) => {
                        let c_name = match CString::new(name) {
                            Ok(s) => s,
                            Err(_) => return ERROR_INVALID_ARGS,
                        };

                        let bytes = c_name.as_bytes_with_nul();
                        if bytes.len() > buffer_size {
                            return ERROR_INVALID_ARGS; // 缓冲区太小
                        }

                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                bytes.as_ptr(),
                                out_buffer as *mut u8,
                                bytes.len()
                            );
                        }
                        ERROR_OK
                    }
                    None => ERROR_OPEN_FAILED, // 串口已断开
                }
            }
            Err(e) => e,
        }
    })
}

// --- 获取可用串口列表 ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_list_ports(
    out_buffer: *mut c_char,
    buffer_size: usize,
    out_count: *mut c_int,
) -> c_int {
    catch_unwind!({
        if out_buffer.is_null() || out_count.is_null() || buffer_size == 0 {
            return ERROR_INVALID_ARGS;
        }

        match serialport::available_ports() {
            Ok(ports) => {
                let count = ports.len();

                // 如果没有串口，返回空字符串
                if count == 0 {
                    unsafe {
                        *out_buffer = 0; // null terminator
                        *out_count = 0;
                    }
                    return ERROR_OK;
                }

                let mut result = String::new();

                for (i, port) in ports.iter().enumerate() {
                    // 检查是否会超出缓冲区（预估）
                    let estimated_size = result.len() + port.port_name.len() + 2; // +2 for ';' and '\0'
                    if estimated_size >= buffer_size {
                        // 缓冲区不足，返回已收集的部分
                        break;
                    }

                    result.push_str(&port.port_name);
                    if i < count - 1 {
                        result.push(';'); // 使用分号分隔
                    }
                }

                let c_result = match CString::new(result) {
                    Ok(s) => s,
                    Err(_) => return ERROR_INVALID_ARGS,
                };

                let bytes = c_result.as_bytes_with_nul();
                if bytes.len() > buffer_size {
                    return ERROR_INVALID_ARGS; // 缓冲区太小
                }

                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        out_buffer as *mut u8,
                        bytes.len()
                    );
                    *out_count = count as c_int;
                }
                ERROR_OK
            }
            Err(_) => ERROR_OPEN_FAILED,
        }
    })
}

// --- 获取串口字节数（可读取的数据量）---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_bytes_to_read(handle: *mut c_void, out_bytes: *mut c_uint) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_bytes.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                match port_guard.bytes_to_read() {
                    Ok(bytes) => {
                        unsafe { *out_bytes = bytes; }
                        ERROR_OK
                    }
                    Err(_) => ERROR_READ_FAILED,
                }
            }
            Err(e) => e,
        }
    })
}

// --- 获取待发送字节数 ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_bytes_to_write(handle: *mut c_void, out_bytes: *mut c_uint) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_bytes.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_port_lock(context) {
            Ok(port_guard) => {
                match port_guard.bytes_to_write() {
                    Ok(bytes) => {
                        unsafe { *out_bytes = bytes; }
                        ERROR_OK
                    }
                    Err(_) => ERROR_WRITE_FAILED,
                }
            }
            Err(e) => e,
        }
    })
}

// ==========================================
// 缓冲区状态查询接口
// ==========================================

/// 缓冲区状态信息（C结构体）
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BufferStatusInfo {
    pub current_size: usize,        // 当前缓冲区大小
    pub max_size: usize,            // 最大缓冲区大小
    pub warning_threshold: usize,   // 警告阈值
    pub overflow_count: usize,      // 溢出次数
    pub bytes_discarded: usize,     // 丢弃的字节数
    pub warning_count: usize,       // 警告次数
    pub usage_percent: f32,         // 使用率百分比
}

/// 获取缓冲区状态信息
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_get_buffer_status(
    handle: *mut c_void,
    out_status: *mut BufferStatusInfo,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_status.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_buffer_lock(context) {
            Ok(buffer_guard) => {
                let current_size = buffer_guard.len();
                let usage_percent = (current_size as f32 / MAX_BUFFER_SIZE as f32) * 100.0;

                let status = BufferStatusInfo {
                    current_size,
                    max_size: MAX_BUFFER_SIZE,
                    warning_threshold: BUFFER_WARNING_THRESHOLD,
                    overflow_count: context.buffer_stats.overflow_count.load(std::sync::atomic::Ordering::Relaxed),
                    bytes_discarded: context.buffer_stats.bytes_discarded.load(std::sync::atomic::Ordering::Relaxed),
                    warning_count: context.buffer_stats.warning_count.load(std::sync::atomic::Ordering::Relaxed),
                    usage_percent,
                };

                unsafe { *out_status = status; }
                ERROR_OK
            }
            Err(e) => e,
        }
    })
}

/// 重置缓冲区统计信息
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_reset_buffer_stats(handle: *mut c_void) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        context.buffer_stats.overflow_count.store(0, std::sync::atomic::Ordering::Relaxed);
        context.buffer_stats.bytes_discarded.store(0, std::sync::atomic::Ordering::Relaxed);
        context.buffer_stats.warning_count.store(0, std::sync::atomic::Ordering::Relaxed);

        ERROR_OK
    })
}

/// 清空内部读取缓冲区
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_clear_buffer(handle: *mut c_void) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        match recover_buffer_lock(context) {
            Ok(mut buffer_guard) => {
                buffer_guard.clear();
                ERROR_OK
            }
            Err(e) => e,
        }
    })
}

// ==========================================
// 原有接口继续
// ==========================================   

//内存释放辅助函数：防止 UE5 内存泄漏
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_free_string(ptr: *mut c_char) {
    if ptr.is_null() { return; }
    unsafe { let _ = CString::from_raw(ptr); }
}

// ==========================================
// 4. JSON 解析接口 (暴露给 UE5)
// ==========================================

/// 从串口读取并解析JSON格式的传感器数据
/// 该函数会尝试从内部缓冲区中提取完整的JSON字符串并解析
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_read_json_sensor_data(
    handle: *mut c_void,
    out_sensor_data: *mut SensorData,
) -> c_int {
    catch_unwind!({
        if handle.is_null() || out_sensor_data.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &*(handle as *mut SerialContext) };

        // 获取缓冲区锁
        let mut buffer_guard = match recover_buffer_lock(context) {
            Ok(g) => g,
            Err(e) => return e,
        };

        // 尝试将缓冲区转换为UTF-8字符串
        let buffer_str = match std::str::from_utf8(&buffer_guard) {
            Ok(s) => s,
            Err(_) => return ERROR_JSON_PARSE_FAILED,
        };

        // 查找完整的JSON对象（从 { 到 }）
        if let Some(start) = buffer_str.find('{') {
            if let Some(end) = buffer_str[start..].find('}') {
                let json_str = &buffer_str[start..start + end + 1];

                // 解析JSON，使用协议配置中的期望点数
                match parse_sensor_json(json_str, context.protocol.expected_points) {
                    Ok(sensor_data) => {
                        unsafe { *out_sensor_data = sensor_data; }

                        // 从缓冲区中移除已解析的数据
                        buffer_guard.drain(0..start + end + 1);

                        return ERROR_OK;
                    }
                    Err(_) => return ERROR_JSON_PARSE_FAILED,
                }
            }
        }

        // 没有找到完整的JSON对象
        ERROR_READ_FAILED
    })
}

/// 直接解析JSON字符串为传感器数据（用于测试或直接解析）
/// expected_points: 期望的点数量，0表示不检查
#[unsafe(no_mangle)]
pub unsafe extern "C" fn parse_json_sensor_data(
    json_str: *const c_char,
    out_sensor_data: *mut SensorData,
    expected_points: u16,
) -> c_int {
    catch_unwind!({
        if json_str.is_null() || out_sensor_data.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let json_string = match c_string_to_rust(json_str) {
            Ok(s) => s,
            Err(_) => return ERROR_INVALID_ARGS,
        };

        match parse_sensor_json(&json_string, expected_points) {
            Ok(sensor_data) => {
                unsafe { *out_sensor_data = sensor_data; }
                ERROR_OK
            }
            Err(_) => ERROR_JSON_PARSE_FAILED,
        }
    })
}

// ==========================================
// 5. 传感器数据访问接口 (暴露给 UE5)
// ==========================================

/// 获取传感器数据中的点数量
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sensor_data_get_count(sensor_data: *const SensorData) -> usize {
    if sensor_data.is_null() {
        return 0;
    }
    unsafe { (*sensor_data).count }
}

/// 获取传感器数据中指定索引的点
/// 返回值：0表示成功，非0表示失败
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sensor_data_get_point(
    sensor_data: *const SensorData,
    index: usize,
    out_point: *mut SensorPoint,
) -> c_int {
    if sensor_data.is_null() || out_point.is_null() {
        return ERROR_INVALID_ARGS;
    }

    let data = unsafe { &*sensor_data };
    if index >= data.count {
        return ERROR_INVALID_ARGS;
    }

    unsafe {
        *out_point = *data.points.add(index);
    }
    ERROR_OK
}

/// 释放传感器数据的内存
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sensor_data_free(sensor_data: *mut SensorData) {
    if sensor_data.is_null() {
        return;
    }

    let data = unsafe { &mut *sensor_data };
    if !data.points.is_null() && data.capacity > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(data.points, data.count, data.capacity);
        }
        data.points = std::ptr::null_mut();
        data.count = 0;
        data.capacity = 0;
    }
}

/// 设置协议配置中的期望点数
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_set_expected_points(
    handle: *mut c_void,
    expected_points: u16,
) -> c_int {
    catch_unwind!({
        if handle.is_null() {
            return ERROR_INVALID_ARGS;
        }

        let context = unsafe { &mut *(handle as *mut SerialContext) };
        context.protocol.expected_points = expected_points;
        ERROR_OK
    })
}

/// 获取协议配置中的期望点数
#[unsafe(no_mangle)]
pub unsafe extern "C" fn serial_get_expected_points(handle: *mut c_void) -> u16 {
    if handle.is_null() {
        return 0;
    }

    let context = unsafe { &*(handle as *mut SerialContext) };
    context.protocol.expected_points
}

