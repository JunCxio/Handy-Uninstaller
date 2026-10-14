$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

function Find-Logo($root, $relative) {
    if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative)) { return '' }
    try {
        $prefix = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
        $path = [IO.Path]::GetFullPath((Join-Path $root $relative))
    } catch { return '' }
    if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { return '' }
    if (Test-Path -LiteralPath $path -PathType Leaf) { return $path }
    $folder = Split-Path $path
    $stem = [IO.Path]::GetFileNameWithoutExtension($path)
    foreach ($suffix in @('.targetsize-32.png', '.targetsize-24.png', '.targetsize-16.png', '.scale-100.png', '.scale-200.png', '.scale-400.png')) {
        $candidate = Join-Path $folder ($stem + $suffix)
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    $match = Get-ChildItem -LiteralPath $folder -Filter "$stem*" -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in '.png', '.jpg', '.jpeg' -and $_.Name -notmatch 'contrast' } |
        Select-Object -First 1
    if ($match) { return $match.FullName }
    return ''
}

Get-AppxPackage |
    Where-Object { -not $_.IsFramework -and -not $_.IsResourcePackage -and -not $_.NonRemovable } |
    ForEach-Object {
        $package = $_
        $manifest = Get-AppxPackageManifest $package -ErrorAction SilentlyContinue
        $logos = @($manifest.Package.Applications.Application | ForEach-Object { $_.VisualElements.Square44x44Logo }) +
            @($manifest.Package.Properties.Logo)
        $icon = ''
        foreach ($logo in $logos) {
            $icon = Find-Logo $package.InstallLocation $logo
            if ($icon) { break }
        }
        @($package.Name, $package.PackageFullName, $package.Version.ToString(), $package.InstallLocation, $icon, $package.PackageFamilyName) -join "`t"
    }
