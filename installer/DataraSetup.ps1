<#
.SYNOPSIS
    Datara Language & Forgen Compiler - Official Graphical Setup Wizard (Python-style)
.DESCRIPTION
    A modern, high-performance GUI installer for the Datara programming language.
    Installs compiler toolchain, standard library, registers PATH, associates .dtr files
    with official icons, and configures VS Code extension.
#>

Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$repoRoot = (Get-Item "$PSScriptRoot\..").FullName
$iconPath = Join-Path $repoRoot "assets\datara.ico"
$logoPath = Join-Path $repoRoot "assets\datara-logo.png"

$Version = "0.1.0"
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
if (Test-Path $cargoTomlPath) {
    $cargoToml = Get-Content $cargoTomlPath -Raw
    if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $matches[1]
    }
}

$defaultInstallDir = Join-Path $env:LOCALAPPDATA "Programs\Datara"

[xml]$xaml = @"
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Datara Language &amp; Forgen Compiler Setup v$Version"
        Height="540" Width="680"
        WindowStartupLocation="CenterScreen"
        ResizeMode="NoResize"
        Background="#0F172A"
        FontFamily="Segoe UI">
    <Window.Resources>
        <Style TargetType="TextBlock">
            <Setter Property="Foreground" Value="#F8FAFC"/>
        </Style>
        <Style TargetType="CheckBox">
            <Setter Property="Foreground" Value="#E2E8F0"/>
            <Setter Property="FontSize" Value="13"/>
            <Setter Property="Margin" Value="0,6,0,6"/>
        </Style>
    </Window.Resources>

    <Grid>
        <!-- Header Banner -->
        <Border Height="88" VerticalAlignment="Top" Background="#1E293B" BorderBrush="#334155" BorderThickness="0,0,0,1">
            <Grid Margin="24,12,24,12">
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="64"/>
                    <ColumnDefinition Width="*"/>
                </Grid.ColumnDefinitions>
                <Image x:Name="HeaderLogo" Grid.Column="0" Width="52" Height="52" HorizontalAlignment="Left"/>
                <StackPanel Grid.Column="1" VerticalAlignment="Center" Margin="12,0,0,0">
                    <TextBlock Text="Datara Setup Wizard" FontSize="20" FontWeight="Bold" Foreground="#38BDF8"/>
                    <TextBlock Text="High-Performance Post-OOP Systems &amp; Application Language" FontSize="12" Foreground="#94A3B8"/>
                </StackPanel>
            </Grid>
        </Border>

        <!-- Main Configuration Panel -->
        <Grid x:Name="ConfigPanel" Margin="32,108,32,80">
            <StackPanel>
                <TextBlock Text="Install Datara $Version (64-bit)" FontSize="18" FontWeight="SemiBold" Margin="0,0,0,8" Foreground="#F1F5F9"/>
                <TextBlock Text="Select installation options and destination directory below." FontSize="13" Foreground="#94A3B8" Margin="0,0,0,16"/>

                <!-- Destination Directory -->
                <TextBlock Text="Installation Folder:" FontSize="13" FontWeight="SemiBold" Foreground="#CBD5E1" Margin="0,0,0,6"/>
                <Grid Margin="0,0,0,18">
                    <Grid.ColumnDefinitions>
                        <ColumnDefinition Width="*"/>
                        <ColumnDefinition Width="90"/>
                    </Grid.ColumnDefinitions>
                    <TextBox x:Name="TxtInstallDir" Grid.Column="0" Height="32" VerticalContentAlignment="Center"
                             Background="#1E293B" Foreground="#F8FAFC" BorderBrush="#475569" Padding="8,0,8,0" FontSize="13"/>
                    <Button x:Name="BtnBrowse" Grid.Column="1" Content="Browse..." Margin="8,0,0,0" Height="32"
                            Background="#334155" Foreground="#F8FAFC" BorderBrush="#475569" FontWeight="SemiBold"/>
                </Grid>

                <!-- Options -->
                <Border Background="#1E293B" CornerRadius="8" Padding="16,12,16,12" BorderBrush="#334155" BorderThickness="1">
                    <StackPanel>
                        <CheckBox x:Name="ChkPath" IsChecked="True" Content="Add Datara (forgen &amp; datara) to PATH environment variable (Recommended)"/>
                        <CheckBox x:Name="ChkAssoc" IsChecked="True" Content="Associate .dtr files with Datara and official icon in Windows Explorer"/>
                        <CheckBox x:Name="ChkVSCode" IsChecked="True" Content="Install Datara Language Extension for VS Code / Cursor / VSCodium"/>
                        <CheckBox x:Name="ChkStdlib" IsChecked="True" Content="Install complete Standard Library (14 modules: math, text, io, net, etc.)"/>
                    </StackPanel>
                </Border>
            </StackPanel>
        </Grid>

        <!-- Progress Panel (Hidden by default) -->
        <Grid x:Name="ProgressPanel" Margin="32,120,32,80" Visibility="Collapsed">
            <StackPanel VerticalAlignment="Center">
                <TextBlock Text="Installing Datara..." FontSize="18" FontWeight="Bold" Margin="0,0,0,12" Foreground="#38BDF8"/>
                <TextBlock x:Name="LblStatus" Text="Preparing files..." FontSize="13" Foreground="#CBD5E1" Margin="0,0,0,16"/>
                <ProgressBar x:Name="PrgBar" Height="14" Minimum="0" Maximum="100" Value="10" Background="#1E293B" Foreground="#38BDF8" BorderBrush="#475569"/>
            </StackPanel>
        </Grid>

        <!-- Success Panel (Hidden by default) -->
        <Grid x:Name="SuccessPanel" Margin="32,110,32,80" Visibility="Collapsed">
            <StackPanel VerticalAlignment="Center">
                <TextBlock Text="✓ Setup was successful" FontSize="22" FontWeight="Bold" Foreground="#4ADE80" Margin="0,0,0,10"/>
                <TextBlock Text="Datara $Version is now ready to use on your system!" FontSize="14" Foreground="#E2E8F0" Margin="0,0,0,16"/>

                <Border Background="#1E293B" CornerRadius="8" Padding="16" BorderBrush="#334155" BorderThickness="1" Margin="0,0,0,16">
                    <StackPanel>
                        <TextBlock Text="Quick Start Commands:" FontWeight="SemiBold" Foreground="#38BDF8" Margin="0,0,0,6"/>
                        <TextBlock Text="• forgen --version" FontFamily="Consolas" Foreground="#F8FAFC" Margin="0,2,0,2"/>
                        <TextBlock Text="• forgen repl" FontFamily="Consolas" Foreground="#F8FAFC" Margin="0,2,0,2"/>
                        <TextBlock Text="• forgen run main.dtr" FontFamily="Consolas" Foreground="#F8FAFC" Margin="0,2,0,2"/>
                    </StackPanel>
                </Border>
                <TextBlock Text="All .dtr files are now associated with the official Datara icon in Windows Explorer." FontSize="12" Foreground="#94A3B8"/>
            </StackPanel>
        </Grid>

        <!-- Bottom Action Bar -->
        <Border Height="68" VerticalAlignment="Bottom" Background="#0B1120" BorderBrush="#1E293B" BorderThickness="0,1,0,0">
            <Grid Margin="24,14,24,14">
                <TextBlock Text="Datara Project • Apache 2.0 / MIT Open Source" FontSize="11" Foreground="#64748B" VerticalAlignment="Center" HorizontalAlignment="Left"/>
                <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">
                    <Button x:Name="BtnCancel" Content="Cancel" Width="90" Height="36" Margin="0,0,10,0"
                            Background="#1E293B" Foreground="#CBD5E1" BorderBrush="#334155"/>
                    <Button x:Name="BtnInstall" Content="Install Now" Width="120" Height="36"
                            Background="#0284C7" Foreground="#FFFFFF" FontWeight="Bold" BorderThickness="0"/>
                    <Button x:Name="BtnFinish" Content="Close" Width="100" Height="36" Visibility="Collapsed"
                            Background="#10B981" Foreground="#FFFFFF" FontWeight="Bold" BorderThickness="0"/>
                </StackPanel>
            </Grid>
        </Border>
    </Grid>
</Window>
"@

$reader = New-Object System.Xml.XmlNodeReader $xaml
$window = [System.Windows.Markup.XamlReader]::Load($reader)

# Element References
$headerLogo   = $window.FindName("HeaderLogo")
$configPanel  = $window.FindName("ConfigPanel")
$progressPanel= $window.FindName("ProgressPanel")
$successPanel = $window.FindName("SuccessPanel")
$txtInstallDir= $window.FindName("TxtInstallDir")
$btnBrowse    = $window.FindName("BtnBrowse")
$btnInstall   = $window.FindName("BtnInstall")
$btnCancel    = $window.FindName("BtnCancel")
$btnFinish    = $window.FindName("BtnFinish")
$lblStatus    = $window.FindName("LblStatus")
$prgBar       = $window.FindName("PrgBar")

$chkPath      = $window.FindName("ChkPath")
$chkAssoc     = $window.FindName("ChkAssoc")
$chkVSCode    = $window.FindName("ChkVSCode")
$chkStdlib    = $window.FindName("ChkStdlib")

$txtInstallDir.Text = $defaultInstallDir

# Load Logo
if (Test-Path $logoPath) {
    $bitmap = New-Object System.Windows.Media.Imaging.BitmapImage
    $bitmap.BeginInit()
    $bitmap.UriSource = New-Object System.Uri($logoPath, [System.UriKind]::Absolute)
    $bitmap.EndInit()
    $headerLogo.Source = $bitmap
}

# Browse Folder
$btnBrowse.Add_Click({
    $fbd = New-Object System.Windows.Forms.FolderBrowserDialog
    $fbd.Description = "Select Datara Installation Folder"
    $fbd.SelectedPath = $txtInstallDir.Text
    if ($fbd.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
        $txtInstallDir.Text = $fbd.SelectedPath
    }
})

# Cancel / Close
$btnCancel.Add_Click({ $window.Close() })
$btnFinish.Add_Click({ $window.Close() })

# Execute Install Routine
$btnInstall.Add_Click({
    $installDir = $txtInstallDir.Text
    $doPath   = $chkPath.IsChecked
    $doAssoc  = $chkAssoc.IsChecked
    $doVSCode = $chkVSCode.IsChecked
    $doStdlib = $chkStdlib.IsChecked

    # Switch to progress view
    $configPanel.Visibility = [System.Windows.Visibility]::Collapsed
    $progressPanel.Visibility = [System.Windows.Visibility]::Visible
    $btnInstall.Visibility = [System.Windows.Visibility]::Collapsed
    $btnCancel.IsEnabled = $false

    $action = {
        param($step, $msg)
        $prgBar.Value = $step
        $lblStatus.Text = $msg
        [System.Windows.Forms.Application]::DoEvents()
    }

    # Step 1: Create Directories
    & $action 15 "Creating installation directories..."
    $binDir = Join-Path $installDir "bin"
    $stdlibDst = Join-Path $installDir "stdlib"
    $assetsDst = Join-Path $installDir "assets"
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    New-Item -ItemType Directory -Force -Path $assetsDst | Out-Null

    # Step 2: Copy Binaries
    & $action 35 "Installing Forgen compiler and Datara runtime..."
    $binCandidates = @(
        (Join-Path $repoRoot "target\release"),
        (Join-Path $repoRoot "dist\staging\bin"),
        (Join-Path $repoRoot "dist\forgen-windows-x64\bin"),
        (Join-Path $repoRoot "dist\forgen-v$Version-windows-x64\bin")
    )
    $distDir = Join-Path $repoRoot "dist"
    if (Test-Path $distDir) {
        Get-ChildItem -Path $distDir -Directory -Filter "forgen*windows*" -ErrorAction SilentlyContinue | ForEach-Object {
            $subBin = Join-Path $_.FullName "bin"
            if (Test-Path $subBin) { $binCandidates += $subBin }
            $binCandidates += $_.FullName
        }
    }
    $binSrc = $binCandidates | Where-Object { Test-Path (Join-Path $_ "forgen.exe") } | Select-Object -First 1

    if (Test-Path (Join-Path $binSrc "forgen.exe")) {
        Copy-Item -Path (Join-Path $binSrc "forgen.exe") -Destination (Join-Path $binDir "forgen.exe") -Force
        Copy-Item -Path (Join-Path $binSrc "forgen.exe") -Destination (Join-Path $binDir "datara.exe") -Force
    }

    # Copy Icon & Assets
    if (Test-Path $iconPath) {
        Copy-Item -Path $iconPath -Destination (Join-Path $assetsDst "datara.ico") -Force
    }
    if (Test-Path $logoPath) {
        Copy-Item -Path $logoPath -Destination (Join-Path $assetsDst "datara-logo.png") -Force
    }

    # Step 3: Copy Standard Library
    if ($doStdlib) {
        & $action 55 "Installing standard library modules..."
        $stdlibSrc = Join-Path $repoRoot "stdlib"
        if (Test-Path $stdlibSrc) {
            Copy-Item -Path $stdlibSrc -Destination $installDir -Recurse -Force
        }
    }

    # Step 4: Register File Associations
    if ($doAssoc) {
        & $action 75 "Registering .dtr file association and official Windows icon..."
        $assocScript = Join-Path $repoRoot "scripts\register_file_associations.ps1"
        if (Test-Path $assocScript) {
            & $assocScript -InstallDir $installDir
        }
    }

    # Step 5: Add to User PATH
    if ($doPath) {
        & $action 88 "Configuring system PATH environment variable..."
        $userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
        if ($userPath -notlike "*$binDir*") {
            $newPath = "$binDir;$userPath"
            [System.Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
            [System.Environment]::SetEnvironmentVariable("DATARA_HOME", $installDir, "User")
        }
    }

    # Step 6: Install VS Code Extension
    if ($doVSCode) {
        & $action 95 "Configuring VS Code syntax extension..."
        $vsScript = Join-Path $repoRoot "scripts\install_vscode_extension.bat"
        if (Test-Path $vsScript) {
            Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"$vsScript`"" -WindowStyle Hidden -Wait
        }
    }

    # Complete!
    & $action 100 "Installation completed successfully!"
    Start-Sleep -Milliseconds 400

    $progressPanel.Visibility = [System.Windows.Visibility]::Collapsed
    $successPanel.Visibility = [System.Windows.Visibility]::Visible
    $btnCancel.Visibility = [System.Windows.Visibility]::Collapsed
    $btnFinish.Visibility = [System.Windows.Visibility]::Visible
})

# Launch Application
$window.ShowDialog() | Out-Null
