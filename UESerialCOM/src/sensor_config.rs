//! Sensor Data Configuration Module
//!
//! Provides blueprint-exposed functions for configuring sensor data parameters.

use std::os::raw::{c_int, c_uint};

/// Create a sensor data structure with custom point capacity
///
/// # Parameters
/// - `capacity`: Number of sensor points (1-256)
///
/// # Returns
/// - Actual capacity used (clamped to 1-256)
#[unsafe(no_mangle)]
pub extern "C" fn sensor_data_create_with_capacity(capacity: c_uint) -> c_uint {
    capacity.clamp(1, 256)
}

/// Validate sensor data point count
///
/// # Parameters
/// - `actual_count`: Actual number of points received
/// - `expected_count`: Expected number of points (0 = accept any)
///
/// # Returns
/// - 1 if valid, 0 if invalid
#[unsafe(no_mangle)]
pub extern "C" fn sensor_data_validate_count(
    actual_count: c_uint,
    expected_count: c_uint,
) -> c_int {
    if expected_count == 0 {
        return if actual_count > 0 && actual_count <= 256 { 1 } else { 0 };
    }

    if actual_count == expected_count && actual_count <= 256 {
        1
    } else {
        0
    }
}

/// Get the default sensor point count (21)
#[unsafe(no_mangle)]
pub extern "C" fn sensor_data_get_default_count() -> c_uint {
    21
}

/// Get the maximum supported sensor point count (256)
#[unsafe(no_mangle)]
pub extern "C" fn sensor_data_get_max_count() -> c_uint {
    256
}
