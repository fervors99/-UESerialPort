using UnrealBuildTool;
using System.IO;

public class UESerialPort : ModuleRules
{
    public UESerialPort(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

        PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "InputCore" });
        PrivateDependencyModuleNames.AddRange(new string[] { "Projects" });

        if (Target.Platform == UnrealTargetPlatform.Win64)
        {
            string BinPath = Path.Combine(ModuleDirectory, "..", "..", "Binaries", "Win64");
            PublicDelayLoadDLLs.Add("ue_serial_port.dll");
            RuntimeDependencies.Add(Path.Combine(BinPath, "ue_serial_port.dll"));
        }
    }
}
