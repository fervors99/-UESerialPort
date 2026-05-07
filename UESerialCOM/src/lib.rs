// UE Serial Port Library - Industrial Grade
// Version: 2.0.0
// License: MIT

mod error;
mod types;
mod buffer;
mod protocol;
mod serial;
mod json_parser;

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use error::{ErrorCode, Result};
use types::{SensorPoint, SensorData, ProtocolConfig, BufferStatusInfo, c_string_to_rust};
use serial::{SerialContext, open_serial_port};
use json_parser::{JsonParser, find_json_object};

// ==========================================
// Panic 捕获宏
// ==========================================

macro_rules! catch_unwind {
    ($body:block) => {
        match panic::catch_unwind(AssertUnwindSafe(|| $body)) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("[FFI] Panic caught: {:?}", e);
                ErrorCode::IoError.as_c_int()
            }
        }
    };
}

// ==========================================
// 句柄管理
// ==========================================

/// 验证句柄有效性
fn validate_handle(handle: *mut c_void) -> Result<&'static SerialContext> {
    if handle.is_null() {
        return Err(ErrorCode::InvalidHandle);
    }

    let context = unsafe { &*(handle as *const SerialContext) };

    if !context.is_valid() {
        return Err(ErrorCode::InvalidHandle);
    }

    Ok(context)
}

/// 将 Arc 转换为原始指针
fn arc_to_raw(arc: Arc<SerialContext>) -> *mut c_void {
    Arc::into_raw(arc) as *mut c_void
}

/// 从原始指针恢复 Arc（用于释放）
unsafe fn raw_to_arc(ptr: *mut c_void) -> Arc<SerialContext> {
    Arc::from_raw(ptr as *const SerialContext)
}

// ==========================================
// FFI 接口 - 基础串口操作
// ==========================================

/// 打开串口
#[no_mangle]
pub unsafe extern "C" fn serial_open(
    port_name: *const c_char,
    baud_rate: c_uint,
    timeout_ms: c_uint,
    out_handle: *mut *mut c_void,
) -> c_int {
    catch_unwind!({
        if port_name.is_null() || out_handle.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let name = match c_string_to_rust(port_name) {
            Ok(s) => s,
            Err(_) => return ErrorCode::InvalidArgs.as_c_int(),
        };

        let timeout = Duration::from_millis(timeout_ms as u64);
        let protocol_config = ProtocolConfig::default();

        match open_serial_port(&name, baud_rate, timeout, protocol_config) {
            Ok(context) => {
                unsafe { *out_handle = arc_to_raw(context); }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

/// 打开串口（带协议配置）
#[no_mangle]
pub unsafe extern "C" fn serial_open_with_config(
    port_name: *const c_char,
    baud_rate: c_uint,
    timeout_ms: c_uint,
    protocol_config: *const ProtocolConfig,
    out_handle: *mut *mut c_void,
) -> c_int {
    catch_unwind!({
        if port_name.is_null() || protocol_config.is_null() || out_handle.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let name = match c_string_to_rust(port_name) {
            Ok(s) => s,
            Err(_) => return ErrorCode::InvalidArgs.as_c_int(),
        };

        let timeout = Duration::from_millis(timeout_ms as u64);
        let config = unsafe { *protocol_config };

        match open_serial_port(&name, baud_rate, timeout, config) {
            Ok(context) => {
                unsafe { *out_handle = arc_to_raw(context); }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

/// 关闭串口
#[no_mangle]
pub unsafe extern "C" fn serial_close(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            let _ = raw_to_arc(handle);
            // Arc 会自动释放
        }
    }
}

/// 写入数据
#[no_mangle]
pub unsafe extern "C" fn serial_write(
    handle: *mut c_void,
    data: *const u8,
    length: usize,
    out_written: *mut usize,
) -> c_int {
    catch_unwind!({
        if data.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        let slice = unsafe { std::slice::from_raw_parts(data, length) };

        match context.write(slice) {
            Ok(n) => {
                if !out_written.is_null() {
                    unsafe { *out_written = n; }
                }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

/// 读取数据
#[no_mangle]
pub unsafe extern "C" fn serial_read(
    handle: *mut c_void,
    buffer: *mut u8,
    length: usize,
    out_read: *mut usize,
) -> c_int {
    catch_unwind!({
        if buffer.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        // 先读取到内部缓冲区
        let _ = context.read_to_buffer();

        // 从内部缓冲区复制数据
        let internal_buffer = match context.lock_buffer() {
            Ok(buf) => buf,
            Err(e) => return e.as_c_int(),
        };

        let available = internal_buffer.as_slice();
        let to_copy = length.min(available.len());

        if to_copy > 0 {
            let dest = unsafe { std::slice::from_raw_parts_mut(buffer, to_copy) };
            dest.copy_from_slice(&available[..to_copy]);
        }

        if !out_read.is_null() {
            unsafe { *out_read = to_copy; }
        }

        ErrorCode::Ok.as_c_int()
    })
}

/// 设置超时
#[no_mangle]
pub unsafe extern "C" fn serial_set_timeout(
    handle: *mut c_void,
    timeout_ms: c_uint,
) -> c_int {
    catch_unwind!({
        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        let timeout = Duration::from_millis(timeout_ms as u64);

        match context.set_timeout(timeout) {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

/// 清空缓冲区
#[no_mangle]
pub unsafe extern "C" fn serial_clear_buffers(
    handle: *mut c_void,
    clear_input: bool,
    clear_output: bool,
) -> c_int {
    catch_unwind!({
        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.clear_buffers(clear_input, clear_output) {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

// ==========================================
// FFI 接口 - 缓冲区管理
// ==========================================

/// 获取缓冲区状态
#[no_mangle]
pub unsafe extern "C" fn serial_get_buffer_status(
    handle: *mut c_void,
    out_status: *mut BufferStatusInfo,
) -> c_int {
    catch_unwind!({
        if out_status.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.get_buffer_status() {
            Ok(status) => {
                unsafe { *out_status = status; }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

/// 重置缓冲区统计
#[no_mangle]
pub unsafe extern "C" fn serial_reset_buffer_stats(handle: *mut c_void) -> c_int {
    catch_unwind!({
        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.reset_buffer_stats() {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

/// 清空内部缓冲区
#[no_mangle]
pub unsafe extern "C" fn serial_clear_buffer(handle: *mut c_void) -> c_int {
    catch_unwind!({
        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.clear_internal_buffer() {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

// ==========================================
// FFI 接口 - JSON 传感器数据
// ==========================================

/// 从串口读取并解析 JSON 传感器数据
#[no_mangle]
pub unsafe extern "C" fn serial_read_json_sensor_data(
    handle: *mut c_void,
    out_sensor_data: *mut SensorData,
) -> c_int {
    catch_unwind!({
        if out_sensor_data.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        // 读取数据到内部缓冲区
        let _ = context.read_to_buffer();

        // 获取缓冲区锁
        let mut buffer_guard = match context.lock_buffer() {
            Ok(g) => g,
            Err(e) => return e.as_c_int(),
        };

        // 查找完整的 JSON 对象
        if let Some((start, end)) = find_json_object(buffer_guard.as_slice()) {
            let json_bytes = &buffer_guard.as_slice()[start..end];
            let json_str = match std::str::from_utf8(json_bytes) {
                Ok(s) => s,
                Err(_) => return ErrorCode::JsonParseFailed.as_c_int(),
            };

            // 获取期望点数
            let expected_points = match context.get_expected_points() {
                Ok(p) => p,
                Err(e) => return e.as_c_int(),
            };

            // 解析 JSON
            let parser = JsonParser::new(expected_points);
            match parser.parse(json_str) {
                Ok(sensor_data) => {
                    unsafe { *out_sensor_data = sensor_data; }
                    buffer_guard.drain(0..end);
                    return ErrorCode::Ok.as_c_int();
                }
                Err(_) => return ErrorCode::JsonParseFailed.as_c_int(),
            }
        }

        ErrorCode::ReadFailed.as_c_int()
    })
}

/// 直接解析 JSON 字符串为传感器数据
#[no_mangle]
pub unsafe extern "C" fn parse_json_sensor_data(
    json_str: *const c_char,
    out_sensor_data: *mut SensorData,
    expected_points: u16,
) -> c_int {
    catch_unwind!({
        if json_str.is_null() || out_sensor_data.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let json_string = match c_string_to_rust(json_str) {
            Ok(s) => s,
            Err(_) => return ErrorCode::InvalidArgs.as_c_int(),
        };

        let parser = JsonParser::new(expected_points);
        match parser.parse(&json_string) {
            Ok(sensor_data) => {
                unsafe { *out_sensor_data = sensor_data; }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

// ==========================================
// FFI 接口 - 传感器数据访问
// ==========================================

/// 获取传感器数据中的点数量
#[no_mangle]
pub unsafe extern "C" fn sensor_data_get_count(sensor_data: *const SensorData) -> usize {
    if sensor_data.is_null() {
        return 0;
    }
    unsafe { (*sensor_data).count }
}

/// 获取指定索引的传感器点
#[no_mangle]
pub unsafe extern "C" fn sensor_data_get_point(
    sensor_data: *const SensorData,
    index: usize,
    out_point: *mut SensorPoint,
) -> c_int {
    catch_unwind!({
        if sensor_data.is_null() || out_point.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let data = unsafe { &*sensor_data };
        if index >= data.count || data.points.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        unsafe {
            *out_point = *data.points.add(index);
        }

        ErrorCode::Ok.as_c_int()
    })
}

/// 释放传感器数据
#[no_mangle]
pub unsafe extern "C" fn sensor_data_free(sensor_data: *mut SensorData) {
    if !sensor_data.is_null() {
        unsafe {
            (*sensor_data).free();
        }
    }
}

// ==========================================
// FFI 接口 - 协议配置
// ==========================================

/// 设置期望点数
#[no_mangle]
pub unsafe extern "C" fn serial_set_expected_points(
    handle: *mut c_void,
    expected_points: u16,
) -> c_int {
    catch_unwind!({
        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.set_expected_points(expected_points) {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

/// 获取期望点数
#[no_mangle]
pub unsafe extern "C" fn serial_get_expected_points(handle: *mut c_void) -> u16 {
    if handle.is_null() {
        return 0;
    }

    let context = match validate_handle(handle) {
        Ok(ctx) => ctx,
        Err(_) => return 0,
    };

    context.get_expected_points().unwrap_or(0)
}

/// 获取协议配置
#[no_mangle]
pub unsafe extern "C" fn serial_get_protocol_config(
    handle: *mut c_void,
    out_config: *mut ProtocolConfig,
) -> c_int {
    catch_unwind!({
        if out_config.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        match context.get_protocol_config() {
            Ok(config) => {
                unsafe { *out_config = config; }
                ErrorCode::Ok.as_c_int()
            }
            Err(e) => e.as_c_int(),
        }
    })
}

/// 设置协议配置
#[no_mangle]
pub unsafe extern "C" fn serial_set_protocol_config(
    handle: *mut c_void,
    config: *const ProtocolConfig,
) -> c_int {
    catch_unwind!({
        if config.is_null() {
            return ErrorCode::InvalidArgs.as_c_int();
        }

        let context = match validate_handle(handle) {
            Ok(ctx) => ctx,
            Err(e) => return e.as_c_int(),
        };

        let protocol_config = unsafe { *config };

        match context.set_protocol_config(protocol_config) {
            Ok(_) => ErrorCode::Ok.as_c_int(),
            Err(e) => e.as_c_int(),
        }
    })
}

// ==========================================
// FFI 接口 - 辅助函数
// ==========================================

/// 释放 Rust 分配的字符串
#[no_mangle]
pub unsafe extern "C" fn rust_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

/// 获取错误描述
#[no_mangle]
pub unsafe extern "C" fn serial_get_error_message(error_code: c_int) -> *mut c_char {
    let message = match ErrorCode::from_c_int(error_code) {
        Some(code) => code.to_string(),
        None => format!("Unknown error code: {}", error_code),
    };

    match CString::new(message) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 获取库版本
#[no_mangle]
pub unsafe extern "C" fn serial_get_version() -> *mut c_char {
    let version = env!("CARGO_PKG_VERSION");
    match CString::new(version) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// ==========================================
// 测试
// ==========================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_conversion() {
        assert_eq!(ErrorCode::Ok.as_c_int(), 0);
        assert_eq!(ErrorCode::InvalidArgs.as_c_int(), 1);
    }

    #[test]
    fn test_sensor_data_lifecycle() {
        let points = vec![
            SensorPoint::new(1.0, 2.0, 3.0),
            SensorPoint::new(4.0, 5.0, 6.0),
        ];
        let mut data = SensorData::new(points);
        assert_eq!(data.count, 2);

        unsafe { data.free(); }
        assert_eq!(data.count, 0);
    }
}
