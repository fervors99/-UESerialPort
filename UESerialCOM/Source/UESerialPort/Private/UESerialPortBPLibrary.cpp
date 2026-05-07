#include "UESerialPortBPLibrary.h"
#include "Misc/Paths.h"
#include "HAL/PlatformProcess.h"

namespace
{
    struct FSerialPortDLL
    {
        // 基础功能
        typedef int32(*TSerialOpen)(const ANSICHAR*, uint32, uint32, void**);
        typedef void(*TSerialClose)(void*);
        typedef int32(*TSerialWrite)(void*, const uint8*, size_t, size_t*);
        typedef int32(*TSerialRead)(void*, uint8*, size_t, size_t*);
        typedef int32(*TSerialSetTimeout)(void*, uint32);

        // 高级功能
        typedef int32(*TSerialIsAlive)(void*);
        typedef int32(*TSerialRestart)(void*, const ANSICHAR*, uint32, uint32);
        typedef int32(*TSerialClearBuffers)(void*, bool, bool);

        // 信息查询
        typedef int32(*TSerialGetPortName)(void*, ANSICHAR*, size_t);
        typedef int32(*TSerialListPorts)(ANSICHAR*, size_t, int32*);
        typedef int32(*TSerialBytesToRead)(void*, uint32*);
        typedef int32(*TSerialBytesToWrite)(void*, uint32*);

        void* DllHandle = nullptr;

        // 基础功能指针
        TSerialOpen Open = nullptr;
        TSerialClose Close = nullptr;
        TSerialWrite Write = nullptr;
        TSerialRead Read = nullptr;
        TSerialSetTimeout SetTimeout = nullptr;

        // 高级功能指针
        TSerialIsAlive IsAlive = nullptr;
        TSerialRestart Restart = nullptr;
        TSerialClearBuffers ClearBuffers = nullptr;

        // 信息查询指针
        TSerialGetPortName GetPortName = nullptr;
        TSerialListPorts ListPorts = nullptr;
        TSerialBytesToRead BytesToRead = nullptr;
        TSerialBytesToWrite BytesToWrite = nullptr;

        bool bLoaded = false;

        bool Load()
        {
            if (bLoaded)
            {
                return true;
            }

            FString DllPath = FPaths::Combine(FPaths::ProjectPluginsDir(), TEXT("UESerialPort/Binaries/Win64/ue_serial_port.dll"));
            if (!FPaths::FileExists(DllPath))
            {
                DllPath = FPaths::Combine(FPaths::ProjectDir(), TEXT("Binaries/Win64/ue_serial_port.dll"));
            }

            if (!FPaths::FileExists(DllPath))
            {
                return false;
            }

            DllHandle = FPlatformProcess::GetDllHandle(*DllPath);
            if (!DllHandle)
            {
                return false;
            }

            // 加载基础功能
            Open = reinterpret_cast<TSerialOpen>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_open")));
            Close = reinterpret_cast<TSerialClose>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_close")));
            Write = reinterpret_cast<TSerialWrite>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_write")));
            Read = reinterpret_cast<TSerialRead>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_read_raw")));
            SetTimeout = reinterpret_cast<TSerialSetTimeout>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_set_timeout")));

            // 加载高级功能
            IsAlive = reinterpret_cast<TSerialIsAlive>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_is_alive")));
            Restart = reinterpret_cast<TSerialRestart>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_restart")));
            ClearBuffers = reinterpret_cast<TSerialClearBuffers>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_clear_buffers")));

            // 加载信息查询功能
            GetPortName = reinterpret_cast<TSerialGetPortName>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_get_port_name")));
            ListPorts = reinterpret_cast<TSerialListPorts>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_list_ports")));
            BytesToRead = reinterpret_cast<TSerialBytesToRead>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_bytes_to_read")));
            BytesToWrite = reinterpret_cast<TSerialBytesToWrite>(FPlatformProcess::GetDllExport(DllHandle, TEXT("serial_bytes_to_write")));

            bLoaded = (Open && Close && Write && Read && SetTimeout);
            if (!bLoaded)
            {
                FPlatformProcess::FreeDllHandle(DllHandle);
                DllHandle = nullptr;
            }

            return bLoaded;
        }

        void Unload()
        {
            if (DllHandle)
            {
                FPlatformProcess::FreeDllHandle(DllHandle);
            }
            DllHandle = nullptr;
            bLoaded = false;
        }
    };

    static FSerialPortDLL SerialDLL;

    bool EnsureLoaded()
    {
        return SerialDLL.Load();
    }
}

// ==========================================
// 基础功能实现
// ==========================================

int64 UUESerialPortBPLibrary::OpenSerialPort(const FString& PortName, int32 BaudRate, int32 TimeoutMs, int32& Result)
{
    Result = -1;
    if (!EnsureLoaded())
    {
        return 0;
    }

    FTCHARToUTF8 Converter(*PortName);
    const ANSICHAR* NameAnsi = Converter.Get();
    void* Handle = nullptr;
    Result = SerialDLL.Open(NameAnsi, static_cast<uint32>(BaudRate), static_cast<uint32>(TimeoutMs), &Handle);
    return reinterpret_cast<int64>(Handle);
}

int32 UUESerialPortBPLibrary::CloseSerialPort(int64 Handle)
{
    if (!EnsureLoaded() || Handle == 0)
    {
        return -1;
    }

    SerialDLL.Close(reinterpret_cast<void*>(Handle));
    return 0;
}

int32 UUESerialPortBPLibrary::WriteSerialPort(int64 Handle, const TArray<uint8>& Data, int32& BytesWritten)
{
    BytesWritten = 0;
    if (!EnsureLoaded() || Handle == 0)
    {
        return -1;
    }

    size_t Written = 0;
    int32 Result = SerialDLL.Write(reinterpret_cast<void*>(Handle), Data.GetData(), Data.Num(), &Written);
    BytesWritten = static_cast<int32>(Written);
    return Result;
}

int32 UUESerialPortBPLibrary::ReadSerialPort(int64 Handle, int32 NumBytes, TArray<uint8>& OutData, int32& BytesRead)
{
    BytesRead = 0;
    OutData.Empty();

    if (!EnsureLoaded() || Handle == 0 || NumBytes <= 0)
    {
        return -1;
    }

    OutData.SetNumZeroed(NumBytes);
    size_t ReadSize = 0;
    int32 Result = SerialDLL.Read(reinterpret_cast<void*>(Handle), OutData.GetData(), OutData.Num(), &ReadSize);
    BytesRead = static_cast<int32>(ReadSize);
    OutData.SetNum(BytesRead);
    return Result;
}

int32 UUESerialPortBPLibrary::SetSerialPortTimeout(int64 Handle, int32 TimeoutMs)
{
    if (!EnsureLoaded() || Handle == 0)
    {
        return -1;
    }

    return SerialDLL.SetTimeout(reinterpret_cast<void*>(Handle), static_cast<uint32>(TimeoutMs));
}

// ==========================================
// 高级功能实现
// ==========================================

bool UUESerialPortBPLibrary::IsSerialPortAlive(int64 Handle)
{
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.IsAlive)
    {
        return false;
    }

    return SerialDLL.IsAlive(reinterpret_cast<void*>(Handle)) == 1;
}

int32 UUESerialPortBPLibrary::RestartSerialPort(int64 Handle, const FString& PortName, int32 BaudRate, int32 TimeoutMs)
{
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.Restart)
    {
        return -1;
    }

    FTCHARToUTF8 Converter(*PortName);
    const ANSICHAR* NameAnsi = Converter.Get();
    return SerialDLL.Restart(reinterpret_cast<void*>(Handle), NameAnsi, static_cast<uint32>(BaudRate), static_cast<uint32>(TimeoutMs));
}

int32 UUESerialPortBPLibrary::ClearSerialBuffers(int64 Handle, bool ClearInput, bool ClearOutput)
{
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.ClearBuffers)
    {
        return -1;
    }

    return SerialDLL.ClearBuffers(reinterpret_cast<void*>(Handle), ClearInput, ClearOutput);
}

// ==========================================
// 信息查询实现
// ==========================================

FString UUESerialPortBPLibrary::GetSerialPortName(int64 Handle, int32& Result)
{
    Result = -1;
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.GetPortName)
    {
        return FString();
    }

    ANSICHAR Buffer[256] = {0};
    Result = SerialDLL.GetPortName(reinterpret_cast<void*>(Handle), Buffer, sizeof(Buffer));

    if (Result == 0)
    {
        return FString(UTF8_TO_TCHAR(Buffer));
    }

    return FString();
}

TArray<FString> UUESerialPortBPLibrary::GetAvailableSerialPorts(int32& Result)
{
    TArray<FString> Ports;
    Result = -1;

    if (!EnsureLoaded() || !SerialDLL.ListPorts)
    {
        return Ports;
    }

    ANSICHAR Buffer[2048] = {0};
    int32 Count = 0;
    Result = SerialDLL.ListPorts(Buffer, sizeof(Buffer), &Count);

    if (Result == 0 && Count > 0)
    {
        FString PortsString = FString(UTF8_TO_TCHAR(Buffer));
        PortsString.ParseIntoArray(Ports, TEXT(";"), true);
    }

    return Ports;
}

int32 UUESerialPortBPLibrary::GetBytesToRead(int64 Handle, int32& OutBytes)
{
    OutBytes = 0;
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.BytesToRead)
    {
        return -1;
    }

    uint32 Bytes = 0;
    int32 Result = SerialDLL.BytesToRead(reinterpret_cast<void*>(Handle), &Bytes);
    OutBytes = static_cast<int32>(Bytes);
    return Result;
}

int32 UUESerialPortBPLibrary::GetBytesToWrite(int64 Handle, int32& OutBytes)
{
    OutBytes = 0;
    if (!EnsureLoaded() || Handle == 0 || !SerialDLL.BytesToWrite)
    {
        return -1;
    }

    uint32 Bytes = 0;
    int32 Result = SerialDLL.BytesToWrite(reinterpret_cast<void*>(Handle), &Bytes);
    OutBytes = static_cast<int32>(Bytes);
    return Result;
}
