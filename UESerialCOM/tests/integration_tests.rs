use ue_serial_port::*;

#[test]
fn test_integration_basic_workflow() {
    // 注意：这个测试需要实际的串口设备或模拟器
    // 在 CI 环境中应该跳过或使用 mock
}

#[test]
fn test_error_handling() {
    // 测试无效参数
    let result = unsafe {
        let mut handle: *mut std::ffi::c_void = std::ptr::null_mut();
        serial_open(
            std::ptr::null(),
            115200,
            1000,
            &mut handle as *mut _,
        )
    };
    assert_ne!(result, 0); // 应该返回错误
}

#[test]
fn test_sensor_data_lifecycle() {
    let json = r#"{"Sx1":"1.0","Sy1":"2.0","Sc1":"3.0","Sx2":"4.0","Sy2":"5.0","Sc2":"6.0"}"#;
    let c_json = std::ffi::CString::new(json).unwrap();

    unsafe {
        let mut sensor_data = std::mem::zeroed();
        let result = parse_json_sensor_data(
            c_json.as_ptr(),
            &mut sensor_data,
            2,
        );

        assert_eq!(result, 0); // 成功
        assert_eq!(sensor_data_get_count(&sensor_data), 2);

        // 获取第一个点
        let mut point = std::mem::zeroed();
        let result = sensor_data_get_point(&sensor_data, 0, &mut point);
        assert_eq!(result, 0);
        assert_eq!(point.x, 1.0);
        assert_eq!(point.y, 2.0);
        assert_eq!(point.c, 3.0);

        // 释放内存
        sensor_data_free(&mut sensor_data);
        assert_eq!(sensor_data.count, 0);
    }
}

#[test]
fn test_version_info() {
    unsafe {
        let version = serial_get_version();
        assert!(!version.is_null());

        let version_str = std::ffi::CStr::from_ptr(version).to_str().unwrap();
        assert!(version_str.starts_with("2."));

        rust_free_string(version);
    }
}

#[test]
fn test_error_messages() {
    unsafe {
        let msg = serial_get_error_message(1); // InvalidArgs
        assert!(!msg.is_null());

        let msg_str = std::ffi::CStr::from_ptr(msg).to_str().unwrap();
        assert!(msg_str.contains("Invalid"));

        rust_free_string(msg);
    }
}

#[cfg(feature = "mock_serial")]
#[test]
fn test_mock_serial_operations() {
    // 使用 mock 串口进行测试
    // 这需要额外的 mock 实现
}
