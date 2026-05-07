#pragma once

#ifdef _WIN32
#    ifdef UE_SERIAL_PORT_EXPORTS
#        define UE_SERIAL_PORT_API extern "C" __declspec(dllexport)
#    else
#        define UE_SERIAL_PORT_API extern "C" __declspec(dllimport)
#    endif
#else
#    define UE_SERIAL_PORT_API extern "C"
#endif

#include <cstdint>
#include <cstddef>

#ifndef UE_SERIAL_PORT_H
#define UE_SERIAL_PORT_H

#ifdef __cplusplus
extern "C" {
#endif

typedef void* SerialHandle;
typedef int32_t SerialResult;

#define SERIAL_OK 0
#define SERIAL_INVALID_ARGS 1
#define SERIAL_OPEN_FAILED 2
#define SERIAL_NOT_FOUND 3
#define SERIAL_WRITE_FAILED 4
#define SERIAL_READ_FAILED 5
#define SERIAL_TIMEOUT_FAILED 6
#define SERIAL_PERMISSION_DENIED 7
#define SERIAL_IO_ERROR 8
#define SERIAL_JSON_PARSE_FAILED 9

// 传感器点数据
typedef struct {
    float x;
    float y;
    float c;
} SensorPoint;

// 传感器数据（动态点数）
typedef struct {
    SensorPoint* points;
    size_t count;
    size_t capacity;
} SensorData;

// 协议配置
typedef struct {
    uint8_t header;
    uint8_t footer;
    uint8_t min_length;
    uint16_t max_length;
    bool use_checksum;
    uint8_t escape_byte;
    uint16_t expected_points;  // 期望的传感器点数量（0表示不检查）
} ProtocolConfig;

// 缓冲区状态信息结构体
typedef struct {
    size_t current_size;
    size_t max_size;
    size_t warning_threshold;
    size_t overflow_count;
    size_t bytes_discarded;
    size_t warning_count;
    float usage_percent;
} BufferStatusInfo;

// 基础串口操作
UE_SERIAL_PORT_API SerialResult serial_open(const char* port_name, uint32_t baud_rate, uint32_t timeout_ms, SerialHandle* out_handle);
UE_SERIAL_PORT_API void serial_close(SerialHandle handle);
UE_SERIAL_PORT_API SerialResult serial_write(SerialHandle handle, const uint8_t* data, size_t length, size_t* out_written);
UE_SERIAL_PORT_API SerialResult serial_read(SerialHandle handle, uint8_t* buffer, size_t length, size_t* out_read);
UE_SERIAL_PORT_API SerialResult serial_set_timeout(SerialHandle handle, uint32_t timeout_ms);
UE_SERIAL_PORT_API SerialResult serial_flush_input(SerialHandle handle);

// 缓冲区管理
UE_SERIAL_PORT_API SerialResult serial_clear_buffers(SerialHandle handle, bool clear_input, bool clear_output);
UE_SERIAL_PORT_API SerialResult serial_get_buffer_status(SerialHandle handle, BufferStatusInfo* out_status);
UE_SERIAL_PORT_API SerialResult serial_reset_buffer_stats(SerialHandle handle);
UE_SERIAL_PORT_API SerialResult serial_clear_buffer(SerialHandle handle);

// JSON 传感器数据解析
UE_SERIAL_PORT_API SerialResult serial_read_json_sensor_data(SerialHandle handle, SensorData* out_sensor_data);
UE_SERIAL_PORT_API SerialResult parse_json_sensor_data(const char* json_str, SensorData* out_sensor_data, uint16_t expected_points);

// 传感器数据访问接口
UE_SERIAL_PORT_API size_t sensor_data_get_count(const SensorData* sensor_data);
UE_SERIAL_PORT_API SerialResult sensor_data_get_point(const SensorData* sensor_data, size_t index, SensorPoint* out_point);
UE_SERIAL_PORT_API void sensor_data_free(SensorData* sensor_data);

// 协议配置接口
UE_SERIAL_PORT_API SerialResult serial_set_expected_points(SerialHandle handle, uint16_t expected_points);
UE_SERIAL_PORT_API uint16_t serial_get_expected_points(SerialHandle handle);

// 内存释放辅助函数
UE_SERIAL_PORT_API void rust_free_string(char* ptr);

#ifdef __cplusplus
}
#endif

#endif // UE_SERIAL_PORT_H