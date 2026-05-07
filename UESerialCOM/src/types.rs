use std::os::raw::c_char;
use std::ffi::CStr;

/// 传感器点数据
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SensorPoint {
    pub x: f32,
    pub y: f32,
    pub c: f32,
}

impl SensorPoint {
    pub fn new(x: f32, y: f32, c: f32) -> Self {
        Self { x, y, c }
    }

    pub fn is_valid(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.c.is_finite()
    }
}

/// 传感器数据（动态点数）
#[repr(C)]
#[derive(Debug)]
pub struct SensorData {
    pub points: *mut SensorPoint,
    pub count: usize,
    pub capacity: usize,
}

impl SensorData {
    pub fn new(points: Vec<SensorPoint>) -> Self {
        let count = points.len();
        let capacity = points.capacity();
        let ptr = Box::into_raw(points.into_boxed_slice()) as *mut SensorPoint;

        Self {
            points: ptr,
            count,
            capacity,
        }
    }

    pub fn empty() -> Self {
        Self {
            points: std::ptr::null_mut(),
            count: 0,
            capacity: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        if self.points.is_null() {
            return self.count == 0 && self.capacity == 0;
        }
        self.count <= self.capacity
    }

    pub unsafe fn as_slice(&self) -> Option<&[SensorPoint]> {
        if self.points.is_null() || self.count == 0 {
            return None;
        }
        Some(std::slice::from_raw_parts(self.points, self.count))
    }

    pub unsafe fn free(&mut self) {
        if !self.points.is_null() && self.capacity > 0 {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(
                self.points,
                self.capacity,
            ));
            self.points = std::ptr::null_mut();
            self.count = 0;
            self.capacity = 0;
        }
    }
}

impl Default for SensorData {
    fn default() -> Self {
        Self::empty()
    }
}

impl Drop for SensorData {
    fn drop(&mut self) {
        unsafe { self.free(); }
    }
}

/// 协议配置
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtocolConfig {
    pub header: u8,
    pub footer: u8,
    pub min_length: u8,
    pub max_length: u16,
    pub use_checksum: bool,
    pub escape_byte: u8,
    pub expected_points: u16,
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
            expected_points: 21,
        }
    }
}

impl ProtocolConfig {
    pub fn validate(&self) -> bool {
        if self.min_length < 2 {
            return false;
        }
        if self.max_length < self.min_length as u16 {
            return false;
        }
        if self.use_checksum && self.min_length < 3 {
            return false;
        }
        if self.header == self.footer && self.header != 0 {
            return false;
        }
        true
    }

    pub fn industrial_preset() -> Self {
        ProtocolConfig {
            header: 0xAA,
            footer: 0x55,
            min_length: 4,
            max_length: 4096,
            use_checksum: true,
            escape_byte: 0x7D,
            expected_points: 0,
        }
    }

    pub fn simple_preset() -> Self {
        ProtocolConfig {
            header: 0xAA,
            footer: 0x55,
            min_length: 4,
            max_length: 256,
            use_checksum: false,
            escape_byte: 0,
            expected_points: 21,
        }
    }
}

/// 缓冲区状态信息
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BufferStatusInfo {
    pub current_size: usize,
    pub max_size: usize,
    pub warning_threshold: usize,
    pub overflow_count: usize,
    pub bytes_discarded: usize,
    pub warning_count: usize,
    pub usage_percent: f32,
}

/// C 字符串转 Rust 字符串
pub fn c_string_to_rust(ptr: *const c_char) -> Result<String, ()> {
    if ptr.is_null() {
        return Err(());
    }
    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map(|s| s.to_owned())
            .map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_point() {
        let point = SensorPoint::new(1.0, 2.0, 3.0);
        assert!(point.is_valid());

        let invalid = SensorPoint::new(f32::NAN, 2.0, 3.0);
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_sensor_data() {
        let points = vec![
            SensorPoint::new(1.0, 2.0, 3.0),
            SensorPoint::new(4.0, 5.0, 6.0),
        ];
        let mut data = SensorData::new(points);
        assert!(data.is_valid());
        assert_eq!(data.count, 2);

        unsafe { data.free(); }
        assert!(data.is_valid());
        assert_eq!(data.count, 0);
    }

    #[test]
    fn test_protocol_config_validation() {
        let valid = ProtocolConfig::default();
        assert!(valid.validate());

        let invalid = ProtocolConfig {
            min_length: 1,
            ..Default::default()
        };
        assert!(!invalid.validate());
    }

    #[test]
    fn test_protocol_presets() {
        let industrial = ProtocolConfig::industrial_preset();
        assert!(industrial.validate());
        assert!(industrial.use_checksum);

        let simple = ProtocolConfig::simple_preset();
        assert!(simple.validate());
        assert!(!simple.use_checksum);
    }
}
