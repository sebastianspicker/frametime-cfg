#Requires -RunAsAdministrator
# ==============================================================================
#  CS2-Optimize-GUI.ps1  —  WPF Dashboard
#  Launch via START-GUI.bat
# ==============================================================================
param([switch]$SmokeTest)

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: CS2-Optimize-GUI" -ForegroundColor Green
    exit 0
}

Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase
Add-Type -AssemblyName System.Windows.Forms   # for Clipboard
Add-Type -AssemblyName Microsoft.VisualBasic  # for InputBox

$Script:Root = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path $MyInvocation.MyCommand.Path -Parent }

. "$Script:Root\config.env.ps1"
. "$Script:Root\helpers.ps1"
. "$Script:Root\helpers\step-catalog.ps1"
. "$Script:Root\helpers\system-analysis.ps1"

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: CS2-Optimize-GUI" -ForegroundColor Green
    exit 0
}

# ── Async engine ──────────────────────────────────────────────────────────────
$Script:Pool   = [System.Management.Automation.Runspaces.RunspaceFactory]::CreateRunspacePool(1, 3)
$Script:Pool.Open()
$Script:UISync    = [hashtable]::Synchronized(@{})
$Script:Closing   = $false
$Script:AsyncTimers = [System.Collections.Generic.List[System.Windows.Threading.DispatcherTimer]]::new()

function Invoke-Async {
    param([scriptblock]$Work, [object[]]$WorkArgs = @(), [scriptblock]$OnDone = {})
    $rs = [System.Management.Automation.PowerShell]::Create()
    $rs.RunspacePool = $Script:Pool
    [void]$rs.AddScript($Work)
    foreach ($a in $WorkArgs) { [void]$rs.AddArgument($a) }
    $handle = $rs.BeginInvoke()
    $timer  = [System.Windows.Threading.DispatcherTimer]::new()
    $timer.Interval = [TimeSpan]::FromMilliseconds(250)
    $capturedHandle = $handle
    $capturedRs     = $rs
    $capturedDone   = $OnDone
    $capturedWindow = $Window
    $capturedUISync = $Script:UISync
    $capturedTimers = $Script:AsyncTimers
    $timer.Add_Tick({
        if ($Script:Closing) {
            $timer.Stop()
            try { $capturedRs.Stop(); $capturedRs.Dispose() } catch {}
            return
        }
        if ($capturedHandle.IsCompleted) {
            $timer.Stop()
            $errorOccurred = $false
            try { $capturedRs.EndInvoke($capturedHandle) } catch { $capturedUISync.AsyncError = "$($_.Exception.GetType().Name): $($_.Exception.Message)"; $errorOccurred = $true }
            finally { $capturedRs.Dispose() }
            if ($errorOccurred) {
                if ($capturedWindow) {
                    $capturedWindow.Dispatcher.Invoke({
                        [System.Windows.MessageBox]::Show("Background task error: $($capturedUISync.AsyncError)", "Error", "OK", "Error")
                    })
                }
                $capturedUISync.AsyncError = $null
            } else {
                try { & $capturedDone } catch {
                    if ($capturedWindow) {
                        $capturedWindow.Dispatcher.Invoke({
                            [System.Windows.MessageBox]::Show("Callback error: $($_.Exception.Message)", "Error", "OK", "Error")
                        })
                    }
                }
            }
            $capturedTimers.Remove($timer)
        }
    }.GetNewClosure())
    $Script:AsyncTimers.Add($timer)
    $timer.Start()
}

function New-Brush { [System.Windows.Media.BrushConverter]::new().ConvertFromString($args[0]) }

# ── XAML ──────────────────────────────────────────────────────────────────────
[xml]$XAML = @'
<Window
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
    Title="CS2 Optimize" Width="1140" Height="720"
    MinWidth="900" MinHeight="560"
    WindowStyle="None" ResizeMode="CanResizeWithGrip"
    Background="#111111"
    FontFamily="Segoe UI, Arial"
    UseLayoutRounding="True"
    SnapsToDevicePixels="True">

  <Window.Resources>
    <SolidColorBrush x:Key="BgMain"    Color="#111111"/>
    <SolidColorBrush x:Key="BgSide"    Color="#0d0d0d"/>
    <SolidColorBrush x:Key="BgCard"    Color="#1c1c1c"/>
    <SolidColorBrush x:Key="BgHeader"  Color="#0d0d0d"/>
    <SolidColorBrush x:Key="Accent"    Color="#e8520a"/>
    <SolidColorBrush x:Key="Success"   Color="#22c55e"/>
    <SolidColorBrush x:Key="Warning"   Color="#fbbf24"/>
    <SolidColorBrush x:Key="Danger"    Color="#ef4444"/>
    <SolidColorBrush x:Key="TextPri"   Color="#f0f0f0"/>
    <SolidColorBrush x:Key="TextMuted" Color="#6b7280"/>
    <SolidColorBrush x:Key="Border"    Color="#2a2a2a"/>

    <Style x:Key="NavBtn" TargetType="Button">
      <Setter Property="Background"              Value="Transparent"/>
      <Setter Property="Foreground"              Value="#6b7280"/>
      <Setter Property="BorderThickness"         Value="0"/>
      <Setter Property="Height"                  Value="42"/>
      <Setter Property="HorizontalContentAlignment" Value="Left"/>
      <Setter Property="Padding"                 Value="18,0,0,0"/>
      <Setter Property="FontSize"                Value="13"/>
      <Setter Property="Cursor"                  Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="{TemplateBinding Background}"
                    BorderThickness="3,0,0,0" BorderBrush="Transparent">
              <ContentPresenter VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#1a1a1a"/>
                <Setter Property="Foreground" Value="#e5e5e5"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="NavBtnActive" TargetType="Button" BasedOn="{StaticResource NavBtn}">
      <Setter Property="Foreground" Value="#f5f5f5"/>
      <Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border Background="#1a1a1a" BorderThickness="3,0,0,0" BorderBrush="#e8520a">
              <ContentPresenter VerticalAlignment="Center"/>
            </Border>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="AccBtn" TargetType="Button">
      <Setter Property="Background"      Value="#e8520a"/>
      <Setter Property="Foreground"      Value="White"/>
      <Setter Property="BorderThickness" Value="0"/>
      <Setter Property="Padding"         Value="18,8"/>
      <Setter Property="FontSize"        Value="12"/>
      <Setter Property="FontWeight"      Value="SemiBold"/>
      <Setter Property="Cursor"          Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="{TemplateBinding Background}"
                    CornerRadius="5" Padding="{TemplateBinding Padding}">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#d04808"/>
              </Trigger>
              <Trigger Property="IsEnabled" Value="False">
                <Setter TargetName="Bd" Property="Background" Value="#333333"/>
                <Setter Property="Foreground" Value="#555"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="SecBtn" TargetType="Button">
      <Setter Property="Background"      Value="#222222"/>
      <Setter Property="Foreground"      Value="#d1d5db"/>
      <Setter Property="BorderBrush"     Value="#383838"/>
      <Setter Property="BorderThickness" Value="1"/>
      <Setter Property="Padding"         Value="14,8"/>
      <Setter Property="FontSize"        Value="12"/>
      <Setter Property="Cursor"          Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="{TemplateBinding Background}"
                    BorderBrush="{TemplateBinding BorderBrush}"
                    BorderThickness="{TemplateBinding BorderThickness}"
                    CornerRadius="5" Padding="{TemplateBinding Padding}">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#2d2d2d"/>
                <Setter TargetName="Bd" Property="BorderBrush" Value="#454545"/>
              </Trigger>
              <Trigger Property="IsEnabled" Value="False">
                <Setter TargetName="Bd" Property="Background" Value="#1a1a1a"/>
                <Setter TargetName="Bd" Property="BorderBrush" Value="#2a2a2a"/>
                <Setter Property="Foreground" Value="#4a4a4a"/>
                <Setter Property="Cursor" Value="Arrow"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="CardBorder" TargetType="Border">
      <Setter Property="Background"       Value="#1a1a1a"/>
      <Setter Property="BorderBrush"      Value="#2a2a2a"/>
      <Setter Property="BorderThickness"  Value="1"/>
      <Setter Property="CornerRadius"     Value="8"/>
      <Setter Property="Padding"          Value="16"/>
    </Style>

    <Style TargetType="DataGrid">
      <Setter Property="Background"               Value="#141414"/>
      <Setter Property="Foreground"               Value="#e0e0e0"/>
      <Setter Property="BorderThickness"          Value="0"/>
      <Setter Property="RowBackground"            Value="#141414"/>
      <Setter Property="AlternatingRowBackground" Value="#191919"/>
      <Setter Property="GridLinesVisibility"      Value="Horizontal"/>
      <Setter Property="HorizontalGridLinesBrush" Value="#222222"/>
      <Setter Property="AutoGenerateColumns"      Value="False"/>
      <Setter Property="CanUserAddRows"           Value="False"/>
      <Setter Property="CanUserDeleteRows"        Value="False"/>
      <Setter Property="SelectionMode"            Value="Single"/>
      <Setter Property="FontSize"                 Value="12"/>
      <Setter Property="RowHeight"                Value="32"/>
    </Style>

    <Style TargetType="DataGridColumnHeader">
      <Setter Property="Background"      Value="#0d0d0d"/>
      <Setter Property="Foreground"      Value="#9ca3af"/>
      <Setter Property="BorderBrush"     Value="#252525"/>
      <Setter Property="BorderThickness" Value="0,0,0,1"/>
      <Setter Property="Padding"         Value="10,8"/>
      <Setter Property="FontSize"        Value="11"/>
      <Setter Property="FontWeight"      Value="SemiBold"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="DataGridColumnHeader">
            <Border Background="{TemplateBinding Background}"
                    BorderBrush="{TemplateBinding BorderBrush}"
                    BorderThickness="{TemplateBinding BorderThickness}"
                    Padding="{TemplateBinding Padding}">
              <Grid>
                <Grid.ColumnDefinitions>
                  <ColumnDefinition Width="*"/>
                  <ColumnDefinition Width="Auto"/>
                </Grid.ColumnDefinitions>
                <ContentPresenter Grid.Column="0"
                                  HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}"
                                  VerticalAlignment="Center"/>
                <Path x:Name="SortArrow" Grid.Column="1"
                      Fill="#6b7280" Margin="4,0,0,0"
                      VerticalAlignment="Center" Visibility="Collapsed"/>
                <Thumb x:Name="PART_RightHeaderGripper" Grid.Column="1"
                       Width="2" HorizontalAlignment="Right" Cursor="SizeWE"
                       Style="{x:Null}">
                  <Thumb.Template>
                    <ControlTemplate TargetType="Thumb">
                      <Border Background="Transparent" Width="2"/>
                    </ControlTemplate>
                  </Thumb.Template>
                </Thumb>
              </Grid>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="SortDirection" Value="Ascending">
                <Setter TargetName="SortArrow" Property="Visibility" Value="Visible"/>
                <Setter TargetName="SortArrow" Property="Data" Value="M 0,5 L 4,0 L 8,5 Z"/>
              </Trigger>
              <Trigger Property="SortDirection" Value="Descending">
                <Setter TargetName="SortArrow" Property="Visibility" Value="Visible"/>
                <Setter TargetName="SortArrow" Property="Data" Value="M 0,0 L 4,5 L 8,0 Z"/>
              </Trigger>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter Property="Background" Value="#161616"/>
                <Setter Property="Foreground" Value="#d1d5db"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style TargetType="DataGridCell">
      <Setter Property="BorderThickness" Value="0"/>
      <Setter Property="Padding"         Value="10,0"/>
      <Setter Property="Foreground"      Value="#e0e0e0"/>
    </Style>

    <Style TargetType="TextBlock">
      <Setter Property="Foreground" Value="#e0e0e0"/>
    </Style>

    <Style TargetType="TextBox">
      <Setter Property="Background"      Value="#1c1c1c"/>
      <Setter Property="Foreground"      Value="#e0e0e0"/>
      <Setter Property="BorderBrush"     Value="#3a3a3a"/>
      <Setter Property="BorderThickness" Value="1"/>
      <Setter Property="Padding"         Value="8,5"/>
      <Setter Property="FontSize"        Value="12"/>
      <Setter Property="CaretBrush"      Value="#e8520a"/>
    </Style>

    <ControlTemplate x:Key="ComboBoxToggle" TargetType="ToggleButton">
      <Grid>
        <Grid.ColumnDefinitions>
          <ColumnDefinition/>
          <ColumnDefinition Width="20"/>
        </Grid.ColumnDefinitions>
        <Border x:Name="Border" Grid.ColumnSpan="2"
                Background="#1c1c1c" BorderBrush="#3a3a3a" BorderThickness="1" CornerRadius="4"/>
        <Path Grid.Column="1" Data="M 0,0 L 4,4 L 8,0 Z" Fill="#6b7280"
              HorizontalAlignment="Center" VerticalAlignment="Center"/>
      </Grid>
      <ControlTemplate.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Setter TargetName="Border" Property="BorderBrush" Value="#555"/>
        </Trigger>
      </ControlTemplate.Triggers>
    </ControlTemplate>

    <Style TargetType="ComboBox">
      <Setter Property="Background"      Value="#1c1c1c"/>
      <Setter Property="Foreground"      Value="#e0e0e0"/>
      <Setter Property="BorderBrush"     Value="#3a3a3a"/>
      <Setter Property="BorderThickness" Value="1"/>
      <Setter Property="Padding"         Value="8,5"/>
      <Setter Property="FontSize"        Value="12"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="ComboBox">
            <Grid>
              <ToggleButton Template="{StaticResource ComboBoxToggle}"
                            IsChecked="{Binding IsDropDownOpen, Mode=TwoWay, RelativeSource={RelativeSource TemplatedParent}}"
                            Focusable="False" ClickMode="Press"/>
              <ContentPresenter IsHitTestVisible="False"
                                Content="{TemplateBinding SelectionBoxItem}"
                                ContentTemplate="{TemplateBinding SelectionBoxItemTemplate}"
                                Margin="10,4,24,4" VerticalAlignment="Center"
                                HorizontalAlignment="Left"/>
              <Popup x:Name="Popup" IsOpen="{TemplateBinding IsDropDownOpen}"
                     Placement="Bottom" Focusable="False" AllowsTransparency="True"
                     PopupAnimation="Slide">
                <Border x:Name="DropDown" Background="#1c1c1c" BorderBrush="#3a3a3a"
                        BorderThickness="1" CornerRadius="4" Padding="0,4"
                        MinWidth="{TemplateBinding ActualWidth}"
                        MaxHeight="{TemplateBinding MaxDropDownHeight}">
                  <ScrollViewer SnapsToDevicePixels="True">
                    <ItemsPresenter/>
                  </ScrollViewer>
                </Border>
              </Popup>
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style TargetType="ComboBoxItem">
      <Setter Property="Background"  Value="Transparent"/>
      <Setter Property="Foreground"  Value="#e0e0e0"/>
      <Setter Property="Padding"     Value="10,6"/>
      <Setter Property="FontSize"    Value="12"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="ComboBoxItem">
            <Border x:Name="Bd" Background="{TemplateBinding Background}"
                    Padding="{TemplateBinding Padding}">
              <ContentPresenter/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsHighlighted" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#2a2a2a"/>
              </Trigger>
              <Trigger Property="IsSelected" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#333"/>
                <Setter Property="Foreground" Value="#f0f0f0"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style TargetType="CheckBox">
      <Setter Property="Foreground" Value="#e0e0e0"/>
      <Setter Property="FontSize"   Value="12"/>
    </Style>

    <Style TargetType="RadioButton">
      <Setter Property="Foreground" Value="#e0e0e0"/>
      <Setter Property="FontSize"   Value="12"/>
    </Style>

    <Style TargetType="ProgressBar">
      <Setter Property="Background" Value="#252525"/>
      <Setter Property="Foreground" Value="#e8520a"/>
      <Setter Property="BorderThickness" Value="0"/>
      <Setter Property="Height"     Value="10"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="ProgressBar">
            <Grid>
              <Border x:Name="PART_Track" Background="{TemplateBinding Background}" CornerRadius="5"/>
              <Border x:Name="PART_Indicator" Background="{TemplateBinding Foreground}" CornerRadius="5" HorizontalAlignment="Left"/>
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="DangerBtn" TargetType="Button">
      <Setter Property="Background"      Value="#2a1515"/>
      <Setter Property="Foreground"      Value="#f87171"/>
      <Setter Property="BorderBrush"     Value="#7f1d1d"/>
      <Setter Property="BorderThickness" Value="1"/>
      <Setter Property="Padding"         Value="12,7"/>
      <Setter Property="FontSize"        Value="12"/>
      <Setter Property="Cursor"          Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="{TemplateBinding Background}"
                    BorderBrush="{TemplateBinding BorderBrush}"
                    BorderThickness="{TemplateBinding BorderThickness}"
                    CornerRadius="4" Padding="{TemplateBinding Padding}">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#3d1c1c"/>
                <Setter TargetName="Bd" Property="BorderBrush" Value="#991b1b"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="WinBtn" TargetType="Button">
      <Setter Property="Background"      Value="Transparent"/>
      <Setter Property="Foreground"      Value="#6b7280"/>
      <Setter Property="BorderThickness" Value="0"/>
      <Setter Property="Width"           Value="46"/>
      <Setter Property="Height"          Value="38"/>
      <Setter Property="FontSize"        Value="11"/>
      <Setter Property="Cursor"          Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="{TemplateBinding Background}">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#1e1e1e"/>
                <Setter Property="Foreground" Value="#d1d5db"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="WinBtnClose" TargetType="Button" BasedOn="{StaticResource WinBtn}">
      <Setter Property="FontSize" Value="12"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="Bd" Background="Transparent">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#c42b1c"/>
                <Setter Property="Foreground" Value="White"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>

    <Style x:Key="SectionHeader" TargetType="TextBlock">
      <Setter Property="FontSize"    Value="10"/>
      <Setter Property="FontWeight"  Value="Bold"/>
      <Setter Property="Foreground"  Value="#6b7280"/>
      <Setter Property="Margin"      Value="0,0,0,10"/>
    </Style>

    <Style TargetType="Separator">
      <Setter Property="Background" Value="#2a2a2a"/>
    </Style>
  </Window.Resources>

  <!-- Root grid: title bar + body -->
  <Grid>
    <Grid.RowDefinitions>
      <RowDefinition Height="38"/>
      <RowDefinition Height="*"/>
    </Grid.RowDefinitions>

    <!-- ── Title bar ──────────────────────────────────────────────────────── -->
    <Border Grid.Row="0" x:Name="TitleBar" Background="#090909" BorderBrush="#1a1a1a" BorderThickness="0,0,0,1">
      <Grid>
        <StackPanel Orientation="Horizontal" VerticalAlignment="Center" Margin="16,0">
          <Border Width="4" Height="16" Background="#e8520a" CornerRadius="2" Margin="0,0,8,0" VerticalAlignment="Center"/>
          <TextBlock Text="CS2" FontSize="14" FontWeight="Bold" Foreground="#e8520a" VerticalAlignment="Center"/>
          <TextBlock Text=" OPTIMIZE" FontSize="14" FontWeight="Bold" Foreground="#f0f0f0" VerticalAlignment="Center"/>
          <TextBlock x:Name="TitleVersion" Text="" FontSize="10" Foreground="#4b5563" VerticalAlignment="Center" Margin="4,1,0,0"/>
        </StackPanel>
        <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">
          <Button x:Name="BtnMin"   Content="─"  Style="{StaticResource WinBtn}"/>
          <Button x:Name="BtnMax"   Content="▢"  Style="{StaticResource WinBtn}"/>
          <Button x:Name="BtnClose" Content="✕"  Style="{StaticResource WinBtnClose}"/>
        </StackPanel>
      </Grid>
    </Border>

    <!-- ── Body: sidebar + content ───────────────────────────────────────── -->
    <Grid Grid.Row="1">
      <Grid.ColumnDefinitions>
        <ColumnDefinition Width="190"/>
        <ColumnDefinition Width="*"/>
      </Grid.ColumnDefinitions>

      <!-- Sidebar -->
      <Border Grid.Column="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,1,0">
        <DockPanel LastChildFill="False">
          <StackPanel DockPanel.Dock="Top" Margin="0,10,0,0">
            <Button x:Name="NavDashboard" Content="⊞   Dashboard"  Style="{StaticResource NavBtnActive}"/>
            <Button x:Name="NavAnalyze"   Content="⌕   Analyze"    Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavOptimize"  Content="⚡   Optimize"   Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavBackup"    Content="⟳   Backup"     Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavBenchmark" Content="◈   Benchmark"  Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavNetwork"   Content="⇄   Network"    Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavVideo"     Content="▣   Video"      Style="{StaticResource NavBtn}"/>
            <Button x:Name="NavSettings"  Content="⚙   Settings"   Style="{StaticResource NavBtn}"/>
          </StackPanel>
          <StackPanel DockPanel.Dock="Bottom" Margin="14,0,14,16">
            <Separator Margin="0,0,0,12"/>
            <TextBlock x:Name="SbProfile" Text="Profile: —"   FontSize="11" Foreground="#9ca3af"/>
            <Border x:Name="SbDryRunBadge" Visibility="Collapsed"
                    Background="#2d1a0a" BorderBrush="#e8520a" BorderThickness="1"
                    CornerRadius="3" Padding="6,3" Margin="0,6,0,0" HorizontalAlignment="Left">
              <TextBlock x:Name="SbDryRun" Text="DRY-RUN" FontSize="10" FontWeight="Bold" Foreground="#e8520a"/>
            </Border>
            <TextBlock x:Name="SbPhase" Text="Phase: —" FontSize="11" Foreground="#6b7280" Margin="0,6,0,0"/>
          </StackPanel>
        </DockPanel>
      </Border>

      <!-- ════ CONTENT PANELS ════ -->
      <Grid Grid.Column="1">

        <!-- ═══ DASHBOARD ═══ -->
        <ScrollViewer x:Name="PanelDashboard" Visibility="Visible" VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="28,20,28,28">
            <TextBlock Text="Dashboard" FontSize="22" FontWeight="SemiBold" Margin="0,0,0,20"/>

            <Border x:Name="DashDriftBanner" Visibility="Collapsed"
                    Background="#1f1408" BorderBrush="#fbbf24" BorderThickness="1"
                    CornerRadius="6" Padding="14" Margin="0,0,0,18">
              <StackPanel>
                <TextBlock x:Name="DashDriftBannerTitle" Text="Configuration Drift Detected"
                           FontSize="12" FontWeight="Bold" Foreground="#fbbf24"/>
                <TextBlock x:Name="DashDriftBannerText" Text=""
                           TextWrapping="Wrap" FontSize="11" Foreground="#d1d5db" Margin="0,4,0,0"/>
              </StackPanel>
            </Border>

            <TextBlock Text="SYSTEM" Style="{StaticResource SectionHeader}"/>
            <UniformGrid Columns="4" Margin="0,0,0,12">
              <Border Style="{StaticResource CardBorder}" Margin="0,0,8,0">
                <StackPanel>
                  <TextBlock Text="CPU" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                  <TextBlock x:Name="CardCpuName"  Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2" TextTrimming="CharacterEllipsis"/>
                  <TextBlock x:Name="CardCpuTier"  Text=""  FontSize="11" Foreground="#e8520a"/>
                  <TextBlock x:Name="CardCpuExtra" Text=""  FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                </StackPanel>
              </Border>
              <Border Style="{StaticResource CardBorder}" Margin="0,0,8,0">
                <StackPanel>
                  <TextBlock Text="GPU" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                  <TextBlock x:Name="CardGpuName"   Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2" TextTrimming="CharacterEllipsis"/>
                  <TextBlock x:Name="CardGpuDriver" Text=""  FontSize="11" Foreground="#6b7280"/>
                  <TextBlock x:Name="CardGpuVendor" Text=""  FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                </StackPanel>
              </Border>
              <Border Style="{StaticResource CardBorder}" Margin="0,0,8,0">
                <StackPanel>
                  <TextBlock Text="RAM" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                  <TextBlock x:Name="CardRamSize"  Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2"/>
                  <TextBlock x:Name="CardRamSpeed" Text=""  FontSize="11" Foreground="#6b7280"/>
                  <TextBlock x:Name="CardRamXmp"   Text=""  FontSize="11" Margin="0,2,0,0"/>
                </StackPanel>
              </Border>
              <Border Style="{StaticResource CardBorder}">
                <StackPanel>
                  <TextBlock Text="NETWORK" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                  <TextBlock x:Name="CardNicName"  Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2" TextTrimming="CharacterEllipsis"/>
                  <TextBlock x:Name="CardNicSpeed" Text=""  FontSize="11" Foreground="#6b7280"/>
                  <TextBlock x:Name="CardNicType"  Text=""  FontSize="11" Margin="0,2,0,0"/>
                </StackPanel>
              </Border>
            </UniformGrid>

            <UniformGrid Columns="2" Margin="0,0,0,22">
              <Border Style="{StaticResource CardBorder}" Margin="0,0,8,0">
                <Grid>
                  <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="*"/>
                    <ColumnDefinition Width="*"/>
                  </Grid.ColumnDefinitions>
                  <StackPanel>
                    <TextBlock Text="OS" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                    <TextBlock x:Name="CardOsName"  Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2"/>
                    <TextBlock x:Name="CardOsBuild" Text=""  FontSize="11" Foreground="#6b7280"/>
                    <TextBlock x:Name="CardOsHags"  Text=""  FontSize="11" Margin="0,2,0,0"/>
                  </StackPanel>
                  <StackPanel Grid.Column="1" Margin="12,0,0,0">
                    <TextBlock Text="CS2" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                    <TextBlock x:Name="CardCs2Status" Text="…" FontSize="12" FontWeight="SemiBold" Margin="0,5,0,2"/>
                    <TextBlock x:Name="CardCs2Cfg"    Text=""  FontSize="11" Foreground="#6b7280"/>
                    <TextBlock x:Name="CardCs2Video"  Text=""  FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </Grid>
              </Border>
              <Border Style="{StaticResource CardBorder}">
                <StackPanel>
                  <TextBlock Text="PERFORMANCE" FontSize="10" FontWeight="Bold" Foreground="#6b7280" Margin="0,0,0,2"/>
                  <TextBlock x:Name="DashPerfBaseline" Text="No benchmark data yet" FontSize="12" Foreground="#6b7280" Margin="0,8,0,0"/>
                  <TextBlock x:Name="DashPerfLatest"   Text=""  FontSize="12" Foreground="#6b7280" Margin="0,3,0,0"/>
                  <TextBlock x:Name="DashPerfDelta"    Text=""  FontSize="16" FontWeight="Bold" Foreground="#22c55e" Margin="0,6,0,0"/>
                </StackPanel>
              </Border>
            </UniformGrid>

            <TextBlock Text="OPTIMIZATION PROGRESS" Style="{StaticResource SectionHeader}"/>
            <Border Style="{StaticResource CardBorder}" Margin="0,0,0,22">
              <StackPanel>
                <Grid Margin="0,0,0,14">
                  <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="75"/>
                    <ColumnDefinition Width="*"/>
                    <ColumnDefinition Width="60"/>
                  </Grid.ColumnDefinitions>
                  <TextBlock Text="Phase 1" FontSize="12" FontWeight="SemiBold" Foreground="#9ca3af" VerticalAlignment="Center"/>
                  <ProgressBar Grid.Column="1" x:Name="ProgressP1" Minimum="0" Maximum="38" Value="0" Margin="10,0" VerticalAlignment="Center"/>
                  <TextBlock Grid.Column="2" x:Name="ProgressP1Txt" Text="0 / 38" FontSize="12" Foreground="#9ca3af" HorizontalAlignment="Right" VerticalAlignment="Center"/>
                </Grid>
                <Grid Margin="0,0,0,14">
                  <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="75"/>
                    <ColumnDefinition Width="*"/>
                    <ColumnDefinition Width="60"/>
                  </Grid.ColumnDefinitions>
                  <TextBlock Text="Phase 2" FontSize="12" FontWeight="SemiBold" Foreground="#9ca3af" VerticalAlignment="Center"/>
                  <ProgressBar Grid.Column="1" x:Name="ProgressP2" Minimum="0" Maximum="3" Value="0" Margin="10,0" VerticalAlignment="Center"/>
                  <TextBlock Grid.Column="2" x:Name="ProgressP2Txt" Text="0 / 3" FontSize="12" Foreground="#9ca3af" HorizontalAlignment="Right" VerticalAlignment="Center"/>
                </Grid>
                <Grid Margin="0,0,0,12">
                  <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="75"/>
                    <ColumnDefinition Width="*"/>
                    <ColumnDefinition Width="60"/>
                  </Grid.ColumnDefinitions>
                  <TextBlock Text="Phase 3" FontSize="12" FontWeight="SemiBold" Foreground="#9ca3af" VerticalAlignment="Center"/>
                  <ProgressBar Grid.Column="1" x:Name="ProgressP3" Minimum="0" Maximum="13" Value="0" Margin="10,0" VerticalAlignment="Center"/>
                  <TextBlock Grid.Column="2" x:Name="ProgressP3Txt" Text="0 / 13" FontSize="12" Foreground="#9ca3af" HorizontalAlignment="Right" VerticalAlignment="Center"/>
                </Grid>
                <TextBlock x:Name="DashIssueHint" Text="" FontSize="12" Foreground="#fbbf24" Margin="0,2,0,0"/>
              </StackPanel>
            </Border>

            <TextBlock Text="QUICK ACTIONS" Style="{StaticResource SectionHeader}"/>
            <WrapPanel>
              <Button x:Name="BtnDashAnalyze"   Content="⌕  Analyze System"    Style="{StaticResource AccBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnDashPhase1"    Content="▶  Run Phase 1"       Style="{StaticResource AccBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnDashVerify"    Content="✓  Verify Settings"   Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnDashBackup"    Content="⟳  Backup Now"        Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnDashLaunchCs2" Content="⚡  Launch CS2"        Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
            </WrapPanel>
          </StackPanel>
        </ScrollViewer>

        <!-- ═══ ANALYZE ═══ -->
        <Grid x:Name="PanelAnalyze" Visibility="Collapsed">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <Border Grid.Row="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1" Padding="28,14">
            <Grid>
              <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
              <StackPanel>
                <TextBlock Text="Analyze System" FontSize="20" FontWeight="SemiBold"/>
                <TextBlock x:Name="AnalyzeScanTime" Text="Not yet scanned" FontSize="11" Foreground="#6b7280" Margin="0,4,0,0"/>
              </StackPanel>
              <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Center">
                <TextBlock x:Name="AnalyzeSummary" Text="" FontSize="13" Foreground="#9ca3af" VerticalAlignment="Center" Margin="0,0,16,0"/>
                <Button x:Name="BtnRunAnalysis" Content="▶  Run Full Scan" Style="{StaticResource AccBtn}"/>
              </StackPanel>
            </Grid>
          </Border>
          <DataGrid Grid.Row="1" x:Name="AnalysisGrid" SelectionUnit="FullRow"
                    CanUserSortColumns="True">
            <DataGrid.Columns>
              <DataGridTextColumn Header="Category"    Binding="{Binding Category}" Width="90"  SortDirection="{x:Null}"/>
              <DataGridTextColumn Header="Group"       Binding="{Binding Group}"    Width="100" SortDirection="{x:Null}"/>
              <DataGridTextColumn Header="Item"        Binding="{Binding Item}"     Width="170" SortDirection="{x:Null}"/>
              <DataGridTextColumn Header="Current"     Binding="{Binding Current}"  Width="130" SortDirection="{x:Null}"/>
              <DataGridTextColumn Header="Recommended" Binding="{Binding Recommended}" Width="120" SortDirection="{x:Null}"/>
              <DataGridTemplateColumn Header="Status"  Width="85" SortMemberPath="Status" SortDirection="{x:Null}">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding StatusLabel}" Foreground="{Binding StatusColor}"
                               FontWeight="SemiBold" VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTextColumn Header="Step"   Binding="{Binding StepRef}" Width="65"  SortDirection="{x:Null}"/>
              <DataGridTextColumn Header="Impact" Binding="{Binding Impact}"  Width="*"   SortDirection="{x:Null}"/>
            </DataGrid.Columns>
          </DataGrid>
          <Border Grid.Row="2" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,1,0,0" Padding="28,12">
            <StackPanel Orientation="Horizontal">
              <Button x:Name="BtnAnalyzeGotoOpt" Content="→  Open in Optimize" Style="{StaticResource AccBtn}" Margin="0,0,10,0"/>
              <Button x:Name="BtnAnalyzeExport"  Content="⤓  Export Report"    Style="{StaticResource SecBtn}"/>
              <Button x:Name="BtnAnalyzeStorageRefresh" Content="↺  Storage Health" Style="{StaticResource SecBtn}" Margin="10,0,10,0"/>
              <Button x:Name="BtnAnalyzeTrimEnable" Content="Enable TRIM" Style="{StaticResource SecBtn}" Margin="0,0,10,0"/>
              <Button x:Name="BtnAnalyzeRetrim" Content="ReTrim…" Style="{StaticResource SecBtn}" Margin="0,0,10,0"/>
              <TextBlock x:Name="AnalyzeStorageHealth" Text="" VerticalAlignment="Center" FontSize="12" Foreground="#9ca3af" TextWrapping="Wrap"/>
            </StackPanel>
          </Border>
        </Grid>

        <!-- ═══ OPTIMIZE ═══ -->
        <Grid x:Name="PanelOptimize" Visibility="Collapsed">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <Border Grid.Row="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1" Padding="28,14">
            <Grid>
              <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
              <StackPanel>
                <TextBlock Text="Optimize" FontSize="20" FontWeight="SemiBold"/>
                <TextBlock Text="Steps run in terminal windows — safe, nothing applied silently" FontSize="11" Foreground="#6b7280" Margin="0,4,0,0"/>
              </StackPanel>
              <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Center">
                <ComboBox x:Name="OptFilterCat"    Width="120" Margin="0,0,8,0"/>
                <ComboBox x:Name="OptFilterStatus" Width="120" Margin="0,0,12,0"/>
                <Button x:Name="BtnOptPhase1" Content="▶  Phase 1" Style="{StaticResource AccBtn}" Margin="0,0,8,0"/>
                <Button x:Name="BtnBootSafeMode" Content="Boot to Safe Mode"
                        ToolTip="Reboot into Safe Mode for GPU driver clean removal"
                        Style="{StaticResource SecBtn}" Margin="0,0,8,0"/>
                <Button x:Name="BtnOptPhase2" Content="Phase 2 (Safe Mode)"
                        ToolTip="Phase 2 runs automatically in Safe Mode after Phase 1 completes"
                        Style="{StaticResource SecBtn}" Margin="0,0,8,0"/>
                <Button x:Name="BtnOptPhase3" Content="▶  Phase 3" Style="{StaticResource SecBtn}"/>
              </StackPanel>
            </Grid>
          </Border>
          <DataGrid Grid.Row="1" x:Name="OptimizeGrid" SelectionUnit="FullRow"
                    CanUserSortColumns="True">
            <DataGrid.Columns>
              <DataGridTextColumn Header="Ph"    Binding="{Binding PhLabel}"    Width="30"/>
              <DataGridTextColumn Header="Step"  Binding="{Binding StepLabel}"  Width="40"/>
              <DataGridTextColumn Header="Cat."  Binding="{Binding Category}"   Width="80"/>
              <DataGridTextColumn Header="Title" Binding="{Binding Title}"      Width="*"/>
              <DataGridTemplateColumn Header="Tier" Width="38" SortMemberPath="TierLabel">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding TierLabel}" Foreground="{Binding TierColor}"
                               FontWeight="SemiBold" VerticalAlignment="Center" Margin="4,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTemplateColumn Header="Risk" Width="80" SortMemberPath="Risk">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding Risk}" Foreground="{Binding RiskColor}"
                               VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTemplateColumn Header="Status" Width="90" SortMemberPath="StatusLabel">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding StatusLabel}" Foreground="{Binding StatusColor}"
                               FontWeight="SemiBold" VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTextColumn Header="Reboot?" Binding="{Binding RebootLabel}" Width="60"/>
              <DataGridTextColumn Header="Expected" Binding="{Binding EstLabel}"   Width="110"/>
            </DataGrid.Columns>
          </DataGrid>
          <Border Grid.Row="2" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,1,0,0" Padding="28,12">
            <StackPanel Orientation="Horizontal">
              <Button x:Name="BtnOptFullSetup" Content="▶  Full Setup (Phase 1 → 2 → 3)" Style="{StaticResource AccBtn}" Margin="0,0,10,0"/>
              <Button x:Name="BtnOptVerify"    Content="✓  Verify All"                   Style="{StaticResource SecBtn}"/>
            </StackPanel>
          </Border>
        </Grid>

        <!-- ═══ BACKUP ═══ -->
        <Grid x:Name="PanelBackup" Visibility="Collapsed">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <Border Grid.Row="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1" Padding="28,14">
            <Grid>
              <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
              <StackPanel>
                <TextBlock Text="Backup &amp; Restore" FontSize="20" FontWeight="SemiBold"/>
                <TextBlock x:Name="BackupSummary" Text="Loading…" FontSize="11" Foreground="#6b7280" Margin="0,4,0,0"/>
              </StackPanel>
              <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Center">
                <Button x:Name="BtnBackupRefresh" Content="↺  Refresh"      Style="{StaticResource SecBtn}" Margin="0,0,8,0"/>
                <Button x:Name="BtnBackupExport"  Content="⤓  Export JSON"  Style="{StaticResource SecBtn}" Margin="0,0,8,0"/>
                <Button x:Name="BtnRestoreAll"    Content="↺  Restore All"  Style="{StaticResource AccBtn}"/>
              </StackPanel>
            </Grid>
          </Border>
          <DataGrid Grid.Row="1" x:Name="BackupGrid" SelectionUnit="FullRow"
                    CanUserSortColumns="True">
            <DataGrid.Columns>
              <DataGridTextColumn Header="Step"     Binding="{Binding Step}"      Width="160"/>
              <DataGridTextColumn Header="Type"     Binding="{Binding Type}"      Width="80"/>
              <DataGridTextColumn Header="Key"      Binding="{Binding Key}"       Width="*"/>
              <DataGridTextColumn Header="Original" Binding="{Binding Original}"  Width="120"/>
              <DataGridTextColumn Header="Timestamp" Binding="{Binding Timestamp}" Width="130"/>
            </DataGrid.Columns>
          </DataGrid>
          <Border Grid.Row="2" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,1,0,0" Padding="28,12">
            <StackPanel Orientation="Horizontal">
              <Button x:Name="BtnRestoreStep" Content="↺  Restore Selected Step" Style="{StaticResource AccBtn}" Margin="0,0,10,0"/>
              <Button x:Name="BtnClearBackup" Content="Clear All Backups"         Style="{StaticResource DangerBtn}"/>
            </StackPanel>
          </Border>
        </Grid>

        <!-- ═══ BENCHMARK ═══ -->
        <Grid x:Name="PanelBenchmark" Visibility="Collapsed">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="200"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <Border Grid.Row="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1" Padding="28,14">
            <Grid>
              <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
              <TextBlock Text="Benchmark &amp; FPS Cap" FontSize="20" FontWeight="SemiBold" VerticalAlignment="Center"/>
              <Button Grid.Column="1" x:Name="BtnBenchAdd" Content="+  Add Result" Style="{StaticResource AccBtn}"/>
            </Grid>
          </Border>
          <Border Grid.Row="1" Background="#141414" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1">
            <Canvas x:Name="BenchChart" Margin="50,16,20,28"/>
          </Border>
          <DataGrid Grid.Row="2" x:Name="BenchGrid" SelectionUnit="FullRow"
                    CanUserSortColumns="True">
            <DataGrid.Columns>
              <DataGridTextColumn Header="#"       Binding="{Binding Index}"    Width="35"/>
              <DataGridTextColumn Header="Date"    Binding="{Binding Date}"     Width="90"/>
              <DataGridTextColumn Header="Label"   Binding="{Binding Label}"    Width="*"/>
              <DataGridTextColumn Header="Avg FPS" Binding="{Binding AvgFps}"   Width="70"/>
              <DataGridTextColumn Header="1% Low"  Binding="{Binding P1Fps}"    Width="65"/>
              <DataGridTemplateColumn Header="Δ Avg" Width="65" SortMemberPath="DeltaAvg">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding DeltaAvg}" Foreground="{Binding DeltaColor}" FontWeight="SemiBold" VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTemplateColumn Header="Δ 1%" Width="65" SortMemberPath="DeltaP1">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding DeltaP1}" Foreground="{Binding DeltaP1Color}" FontWeight="SemiBold" VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
            </DataGrid.Columns>
          </DataGrid>
          <!-- FPS Cap bar -->
          <Border Grid.Row="3" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,1,0,0" Padding="28,12">
            <Grid>
              <Grid.ColumnDefinitions>
                <ColumnDefinition Width="Auto"/>
                <ColumnDefinition Width="240"/>
                <ColumnDefinition Width="Auto"/>
                <ColumnDefinition Width="Auto"/>
                <ColumnDefinition Width="Auto"/>
                <ColumnDefinition Width="*"/>
                <ColumnDefinition Width="Auto"/>
              </Grid.ColumnDefinitions>
              <TextBlock Grid.Column="0" Text="FPS Cap" FontSize="13" FontWeight="SemiBold" VerticalAlignment="Center" Margin="0,0,12,0"/>
              <TextBox   Grid.Column="1" x:Name="BenchVprof" ToolTip="Paste [VProf] FPS: Avg=… line here"/>
              <Button    Grid.Column="2" x:Name="BtnBenchParse" Content="Parse" Style="{StaticResource SecBtn}" Margin="10,0"/>
              <TextBlock Grid.Column="3" x:Name="BenchCapLabel" Text="" VerticalAlignment="Center" FontSize="12" Foreground="#9ca3af" Margin="16,0,8,0"/>
              <TextBlock Grid.Column="4" x:Name="BenchCapValue" Text="" VerticalAlignment="Center" FontSize="22" FontWeight="Bold" Foreground="#e8520a" Margin="0,0,16,0"/>
              <Button    Grid.Column="6" x:Name="BtnBenchCopy" Content="Copy Cap" Style="{StaticResource SecBtn}"/>
            </Grid>
          </Border>
        </Grid>

        <!-- ═══ NETWORK ═══ -->
        <ScrollViewer x:Name="PanelNetwork" Visibility="Collapsed" VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="28,20,28,28">
            <TextBlock Text="Valve Region Latency Diagnostic" FontSize="22" FontWeight="SemiBold" Margin="0,0,0,10"/>
            <TextBlock Text="Diagnostic proxy for route quality and relay reachability. This is not a guaranteed in-match CS2 ping." Foreground="#9ca3af" FontSize="12" TextWrapping="Wrap" Margin="0,0,0,18"/>

            <TextBlock Text="NETWORK SUMMARY" Style="{StaticResource SectionHeader}"/>
            <Border Style="{StaticResource CardBorder}" Margin="0,0,0,18">
              <StackPanel>
                <TextBlock x:Name="NetDiagAdapterSummary" Text="Adapter: loading…" FontSize="12" FontWeight="SemiBold" Margin="0,0,0,6"/>
                <TextBlock x:Name="NetDiagDnsSummary" Text="DNS: loading…" FontSize="12" Foreground="#9ca3af"/>
                <TextBlock x:Name="NetDiagHistorySummary" Text="" FontSize="11" Foreground="#6b7280" Margin="0,8,0,0"/>
              </StackPanel>
            </Border>

            <TextBlock Text="DNS WORKFLOW" Style="{StaticResource SectionHeader}"/>
            <WrapPanel Margin="0,0,0,18">
              <Button x:Name="BtnNetRefresh" Content="↺  Refresh" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetBaseline" Content="Baseline Test" Style="{StaticResource AccBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetPost" Content="Post-Change Retest" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetDnsCloudflare" Content="Use Cloudflare" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetDnsGoogle" Content="Use Google" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetDnsDhcp" Content="Reset to DHCP" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
              <Button x:Name="BtnNetDnsRestore" Content="Restore Previous DNS" Style="{StaticResource SecBtn}" Margin="0,0,10,10"/>
            </WrapPanel>

            <TextBlock Text="LATEST BASELINE VS POST" Style="{StaticResource SectionHeader}"/>
            <DataGrid x:Name="NetDiagComparisonGrid" SelectionUnit="FullRow" CanUserSortColumns="True" Margin="0,0,0,18">
              <DataGrid.Columns>
                <DataGridTextColumn Header="Region" Binding="{Binding TargetLabel}" Width="110"/>
                <DataGridTextColumn Header="Baseline Avg" Binding="{Binding BaselineAvgMs}" Width="95"/>
                <DataGridTextColumn Header="Post Avg" Binding="{Binding PostAvgMs}" Width="85"/>
                <DataGridTextColumn Header="Δ ms" Binding="{Binding DeltaMs}" Width="70"/>
                <DataGridTextColumn Header="Timeouts" Binding="{Binding TimeoutSummary}" Width="90"/>
                <DataGridTextColumn Header="Protocol" Binding="{Binding ProtocolUsed}" Width="70"/>
                <DataGridTextColumn Header="Endpoint" Binding="{Binding Endpoint}" Width="*"/>
              </DataGrid.Columns>
            </DataGrid>

            <TextBlock Text="RUN HISTORY" Style="{StaticResource SectionHeader}"/>
            <DataGrid x:Name="NetDiagHistoryGrid" SelectionUnit="FullRow" CanUserSortColumns="True">
              <DataGrid.Columns>
                <DataGridTextColumn Header="Timestamp" Binding="{Binding Timestamp}" Width="135"/>
                <DataGridTextColumn Header="Kind" Binding="{Binding Kind}" Width="70"/>
                <DataGridTextColumn Header="Adapter" Binding="{Binding AdapterName}" Width="120"/>
                <DataGridTextColumn Header="DNS" Binding="{Binding DnsProvider}" Width="90"/>
                <DataGridTextColumn Header="Avg RTT" Binding="{Binding AvgRttMs}" Width="80"/>
                <DataGridTextColumn Header="Regions OK" Binding="{Binding RegionsOk}" Width="80"/>
              </DataGrid.Columns>
            </DataGrid>
          </StackPanel>
        </ScrollViewer>

        <!-- ═══ VIDEO ═══ -->
        <Grid x:Name="PanelVideo" Visibility="Collapsed">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <Border Grid.Row="0" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,0,0,1" Padding="28,14">
            <Grid>
              <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
              <StackPanel>
                <TextBlock Text="Video Settings" FontSize="20" FontWeight="SemiBold"/>
                <TextBlock x:Name="VideoTxtPath" Text="Searching for video.txt…" FontSize="11" Foreground="#6b7280" Margin="0,4,0,0"/>
              </StackPanel>
              <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Center">
                <TextBlock Text="Tier:" FontSize="12" Foreground="#9ca3af" VerticalAlignment="Center" Margin="0,0,8,0"/>
                <ComboBox x:Name="VideoTierPicker" Width="110" Margin="0,0,12,0"/>
                <Button   x:Name="BtnVideoWrite"   Content="Write video.txt" Style="{StaticResource AccBtn}"/>
              </StackPanel>
            </Grid>
          </Border>
          <DataGrid Grid.Row="1" x:Name="VideoGrid" SelectionUnit="FullRow"
                    CanUserSortColumns="True">
            <DataGrid.Columns>
              <DataGridTextColumn Header="Setting"     Binding="{Binding Setting}"     Width="*"/>
              <DataGridTextColumn Header="Your Value"  Binding="{Binding YourValue}"   Width="120"/>
              <DataGridTextColumn Header="Recommended" Binding="{Binding Recommended}" Width="120"/>
              <DataGridTemplateColumn Header="Status"  Width="80" SortMemberPath="StatusLabel">
                <DataGridTemplateColumn.CellTemplate>
                  <DataTemplate>
                    <TextBlock Text="{Binding StatusLabel}" Foreground="{Binding StatusColor}"
                               FontWeight="SemiBold" VerticalAlignment="Center" Margin="6,0"/>
                  </DataTemplate>
                </DataGridTemplateColumn.CellTemplate>
              </DataGridTemplateColumn>
              <DataGridTextColumn Header="Notes" Binding="{Binding Notes}" Width="220"/>
            </DataGrid.Columns>
          </DataGrid>
          <Border Grid.Row="2" Background="#0d0d0d" BorderBrush="#1e1e1e" BorderThickness="0,1,0,0" Padding="28,12">
            <StackPanel Orientation="Horizontal">
              <TextBlock x:Name="VideoSummary" Text="" VerticalAlignment="Center" Margin="0,0,18,0" FontSize="12" Foreground="#9ca3af"/>
              <Button x:Name="BtnVideoWriteFooter" Content="Write video.txt  (renames original → .bak)" Style="{StaticResource AccBtn}"/>
            </StackPanel>
          </Border>
        </Grid>

        <!-- ═══ SETTINGS ═══ -->
        <ScrollViewer x:Name="PanelSettings" Visibility="Collapsed" VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="28,20,28,28" MaxWidth="620">
            <TextBlock Text="Settings" FontSize="22" FontWeight="SemiBold" Margin="0,0,0,22"/>

            <TextBlock Text="PROFILE" Style="{StaticResource SectionHeader}"/>
            <Border Style="{StaticResource CardBorder}" Margin="0,0,0,22">
              <StackPanel>
                <RadioButton x:Name="RadioSafe" GroupName="Profile" Margin="0,0,0,12">
                  <StackPanel>
                    <TextBlock Text="Safe" FontWeight="SemiBold" FontSize="13" Foreground="#22c55e"/>
                    <TextBlock Text="Proven T1 tweaks only. Auto-applied. No risk." FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </RadioButton>
                <RadioButton x:Name="RadioRecommended" GroupName="Profile" Margin="0,0,0,12" IsChecked="True">
                  <StackPanel>
                    <TextBlock Text="Recommended" FontWeight="SemiBold" FontSize="13" Foreground="#e8520a"/>
                    <TextBlock Text="T1 + T2 moderate tweaks with confirmation prompts." FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </RadioButton>
                <RadioButton x:Name="RadioCompetitive" GroupName="Profile" Margin="0,0,0,12">
                  <StackPanel>
                    <TextBlock Text="Competitive" FontWeight="SemiBold" FontSize="13" Foreground="#fbbf24"/>
                    <TextBlock Text="All tiers. Everything the suite offers." FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </RadioButton>
                <RadioButton x:Name="RadioCustom" GroupName="Profile" Margin="0,0,0,12">
                  <StackPanel>
                    <TextBlock Text="Custom" FontWeight="SemiBold" FontSize="13" Foreground="#9ca3af"/>
                    <TextBlock Text="Full detail card for every step. Manual approval." FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </RadioButton>
                <RadioButton x:Name="RadioYolo" GroupName="Profile">
                  <StackPanel>
                    <TextBlock Text="YOLO" FontWeight="SemiBold" FontSize="13" Foreground="#ef4444"/>
                    <TextBlock Text="Everything auto-executes. Zero prompts. GPU auto-detected." FontSize="11" Foreground="#6b7280" Margin="0,2,0,0"/>
                  </StackPanel>
                </RadioButton>
              </StackPanel>
            </Border>

            <Border Background="#1e1608" BorderBrush="#e8520a" BorderThickness="1" CornerRadius="6" Padding="14" Margin="0,0,0,22">
              <CheckBox x:Name="ChkDryRun">
                <StackPanel>
                  <TextBlock Text="DRY-RUN MODE" FontWeight="Bold" FontSize="12" Foreground="#e8520a"/>
                  <TextBlock Text="Preview all changes without applying anything" FontSize="11" Foreground="#9ca3af" Margin="0,2,0,0"/>
                </StackPanel>
              </CheckBox>
            </Border>

            <TextBlock Text="FIRST-TIME SETUP" Style="{StaticResource SectionHeader}"/>
            <Border Style="{StaticResource CardBorder}" Margin="0,0,0,22">
              <StackPanel>
                <TextBlock TextWrapping="Wrap" Foreground="#9ca3af" FontSize="12" Margin="0,0,0,14"
                           Text="First-time optimization requires all 3 phases in sequence. Phases 1 → 2 → 3 require reboots and run in terminal. Phase 2 runs in Safe Mode for GPU driver replacement."/>
                <Button x:Name="BtnSettingsPhase1" Content="▶  Launch Full Setup (Phase 1)" Style="{StaticResource AccBtn}" HorizontalAlignment="Left"/>
              </StackPanel>
            </Border>

            <Border Style="{StaticResource CardBorder}">
              <StackPanel>
                <StackPanel Orientation="Horizontal">
                  <Border Width="3" Height="14" Background="#e8520a" CornerRadius="1" Margin="0,0,8,0" VerticalAlignment="Center"/>
                  <TextBlock Text="CS2 Optimization Suite" FontSize="13" FontWeight="SemiBold" VerticalAlignment="Center"/>
                  <TextBlock x:Name="SettingsVersion" Text="" FontSize="11" Foreground="#6b7280" VerticalAlignment="Center"/>
                </StackPanel>
                <TextBlock Text="MIT License" FontSize="11" Foreground="#6b7280" Margin="0,6,0,0"/>
              </StackPanel>
            </Border>
          </StackPanel>
        </ScrollViewer>

      </Grid><!-- end content panels -->
    </Grid><!-- end body grid -->
  </Grid><!-- end root grid -->
</Window>
'@

# ── Load window ───────────────────────────────────────────────────────────────
$reader = [System.Xml.XmlNodeReader]::new($XAML)
$Window = [Windows.Markup.XamlReader]::Load($reader)
$reader.Dispose()

# ── Named element shortcuts ───────────────────────────────────────────────────
function El {
    $e = $Window.FindName($args[0])
    if ($null -eq $e) { Write-Warning "El: XAML element '$($args[0])' not found" }
    $e
}

# ── Version labels (from config.env.ps1) ─────────────────────────────────────
(El "TitleVersion").Text    = "  $CFG_Version"
(El "SettingsVersion").Text = "  $CFG_Version"

# ── Window chrome ─────────────────────────────────────────────────────────────
(El "TitleBar").Add_MouseLeftButtonDown({ $Window.DragMove() })
(El "BtnMin").Add_Click({ $Window.WindowState = "Minimized" })
(El "BtnMax").Add_Click({ $Window.WindowState = if ($Window.WindowState -eq "Maximized") { "Normal" } else { "Maximized" } })
(El "BtnClose").Add_Click({ $Window.Close() })

# ── Navigation ────────────────────────────────────────────────────────────────
$Script:AllPanels = "PanelDashboard","PanelAnalyze","PanelOptimize","PanelBackup","PanelBenchmark","PanelNetwork","PanelVideo","PanelSettings"
$Script:NavMap    = @{
    "PanelDashboard"  = "NavDashboard"
    "PanelAnalyze"    = "NavAnalyze"
    "PanelOptimize"   = "NavOptimize"
    "PanelBackup"     = "NavBackup"
    "PanelBenchmark"  = "NavBenchmark"
    "PanelNetwork"    = "NavNetwork"
    "PanelVideo"      = "NavVideo"
    "PanelSettings"   = "NavSettings"
}
$Script:ActivePanel = "PanelDashboard"

$ActiveStyle   = $Window.Resources["NavBtnActive"]
$InactiveStyle = $Window.Resources["NavBtn"]

(El "NavDashboard").Add_Click({ Switch-Panel "PanelDashboard"; Load-Dashboard })
(El "NavAnalyze"  ).Add_Click({ Switch-Panel "PanelAnalyze" ; Start-Analysis })
(El "NavOptimize" ).Add_Click({ Switch-Panel "PanelOptimize" ; Load-Optimize  })
(El "NavBackup"   ).Add_Click({ Switch-Panel "PanelBackup"   ; Load-Backup    })
(El "NavBenchmark").Add_Click({ Switch-Panel "PanelBenchmark"; Load-Benchmark })
(El "NavNetwork"  ).Add_Click({ Switch-Panel "PanelNetwork"  ; Load-NetworkDiagnostics })
(El "NavVideo"    ).Add_Click({ Switch-Panel "PanelVideo"    ; Load-Video     })
(El "NavSettings" ).Add_Click({ Switch-Panel "PanelSettings" ; Load-Settings  })

# ── Sidebar status helpers ────────────────────────────────────────────────────
function Update-SidebarStatus {
    $state = $null
    try { if (Test-Path $CFG_StateFile) { $state = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop } } catch {}
    $prof = if ($state) { $state.profile } else { "—" }
    $isDry = ($state -and $state.mode -eq "DRY-RUN")
    $phaseText = "—"
    if (Test-Path $CFG_ProgressFile) {
        try {
            $prog = Get-Content $CFG_ProgressFile -Raw | ConvertFrom-Json
            if ($prog.phase) { $phaseText = "$($prog.phase)" }
        } catch {}
    }
    $Window.Dispatcher.Invoke({
        (El "SbProfile").Text = "Profile: $prof"
        (El "SbDryRun" ).Text = if ($isDry) { "DRY-RUN" } else { "" }
        (El "SbDryRunBadge").Visibility = if ($isDry) { "Visible" } else { "Collapsed" }
        (El "SbPhase").Text = "Phase: $phaseText"
    })
}

# ── Load panel functions and event handlers ─────────────────────────────────
. "$Script:Root\helpers\gui-panels.ps1"

# ══════════════════════════════════════════════════════════════════════════════
# STARTUP
# ══════════════════════════════════════════════════════════════════════════════
$Window.Add_Loaded({
    Update-SidebarStatus
    Update-StartupDriftBanner
    Load-Dashboard
})

$Window.Add_Closed({
    $Script:Closing = $true
    # Snapshot the list before iterating — Tick handlers call Remove($timer) on this
    # same list, which would throw InvalidOperationException during enumeration.
    $timersSnapshot = @($Script:AsyncTimers)
    foreach ($t in $timersSnapshot) { try { $t.Stop() } catch {} }
    try { $Script:Pool.Close(); $Script:Pool.Dispose() } catch {}
})

$Window.ShowDialog() | Out-Null
