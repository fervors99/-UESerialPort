#pragma once

#ifdef _WIN32
#    ifdef UE_SERIAL_PORT_EXPORTS
#        define UE_UDP_API extern "C" __declspec(dllexport)
#    else
#        define UE_UDP_API extern "C" __declspec(dllimport)
#    endif
#else
#    define UE_UDP_API extern "C"
#endif

#include <cstdint>
#include <cstddef>

using UdpHandle = void*;
using UdpResult = int32_t;

static const UdpResult UDP_OK = 0;
static const UdpResult UDP_INVALID_ARGS = 1;
static const UdpResult UDP_OPEN_FAILED = 2;
static const UdpResult UDP_INVALID_HANDLE = 3;
static const UdpResult UDP_WRITE_FAILED = 4;
static const UdpResult UDP_READ_FAILED = 5;
static const UdpResult UDP_TIMEOUT_FAILED = 6;

UE_UDP_API UdpResult udp_open(const char* bind_addr, UdpHandle* out_handle);
UE_UDP_API void udp_close(UdpHandle handle);
UE_UDP_API UdpResult udp_send(UdpHandle handle, const uint8_t* data, size_t length, const char* target_addr, size_t* out_sent);
UE_UDP_API UdpResult udp_receive(UdpHandle handle, uint8_t* buffer, size_t length, size_t* out_read, char* source_addr, size_t addr_len);
UE_UDP_API UdpResult udp_set_read_timeout(UdpHandle handle, uint32_t timeout_ms);