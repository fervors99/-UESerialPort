#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "UESerialPortBPLibrary.generated.h"

UCLASS()
class UESERIALPORT_API UUESerialPortBPLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:
    // 基础功能
    UFUNCTION(BlueprintCallable, Category = "SerialPort|Basic")
    static int64 OpenSerialPort(const FString& PortName, int32 BaudRate, int32 TimeoutMs, int32& Result);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Basic")
    static int32 CloseSerialPort(int64 Handle);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Basic")
    static int32 WriteSerialPort(int64 Handle, const TArray<uint8>& Data, int32& BytesWritten);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Basic")
    static int32 ReadSerialPort(int64 Handle, int32 NumBytes, TArray<uint8>& OutData, int32& BytesRead);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Basic")
    static int32 SetSerialPortTimeout(int64 Handle, int32 TimeoutMs);

    // 高级功能 - 健康检查与重启
    UFUNCTION(BlueprintCallable, Category = "SerialPort|Advanced")
    static bool IsSerialPortAlive(int64 Handle);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Advanced")
    static int32 RestartSerialPort(int64 Handle, const FString& PortName, int32 BaudRate, int32 TimeoutMs);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Advanced")
    static int32 ClearSerialBuffers(int64 Handle, bool ClearInput, bool ClearOutput);

    // 信息查询
    UFUNCTION(BlueprintCallable, Category = "SerialPort|Info")
    static FString GetSerialPortName(int64 Handle, int32& Result);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Info")
    static TArray<FString> GetAvailableSerialPorts(int32& Result);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Info")
    static int32 GetBytesToRead(int64 Handle, int32& OutBytes);

    UFUNCTION(BlueprintCallable, Category = "SerialPort|Info")
    static int32 GetBytesToWrite(int64 Handle, int32& OutBytes);
};
