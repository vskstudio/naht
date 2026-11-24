param(
    [Parameter(Mandatory = $true)][string]$Naht,
    [Parameter(Mandatory = $true)][string]$Work
)

$ErrorActionPreference = 'Stop'
$script:Failures = @()
$script:Port = 35100
$Other = 'nahtother'
$Me = "$env:USERDOMAIN\$env:USERNAME"
$MySid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User

function Fail([string]$Case, [string]$Why) {
    Write-Host "FAIL [$Case] $Why"
    $script:Failures += "$Case : $Why"
}

function Pass([string]$Case, [string]$What) {
    Write-Host "PASS [$Case] $What"
}

function Invoke-Serve([string]$Root) {
    $script:Port += 1
    $out = Join-Path $Work "serve-$($script:Port).out"
    $err = Join-Path $Work "serve-$($script:Port).err"
    $process = Start-Process -FilePath $Naht -ArgumentList @('serve', $Root, '--port', $script:Port) `
        -RedirectStandardOutput $out -RedirectStandardError $err -PassThru -NoNewWindow
    $deadline = (Get-Date).AddSeconds(30)
    $serving = $false
    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited) { break }
        if ((Test-Path $err) -and (Select-String -Path $err -SimpleMatch 'naht serving' -Quiet)) {
            $serving = $true
            break
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit()
    }
    $text = ((Get-Content $out -Raw -ErrorAction SilentlyContinue) + (Get-Content $err -Raw -ErrorAction SilentlyContinue))
    [pscustomobject]@{ Serving = $serving; Text = "$text" }
}

function New-Project([string]$Root) {
    & $Naht init $Root | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "naht init $Root failed" }
    $first = Invoke-Serve $Root
    if (-not $first.Serving) { throw "first naht serve in $Root did not start: $($first.Text)" }
    (Get-Content (Join-Path $Root '.naht\token') -Raw).Trim()
}

function Get-Acl-Text([string]$Path) {
    (& icacls $Path | Out-String)
}

function Test-OwnerOnly([string]$Case, [string]$Path) {
    try { $acl = Get-Acl $Path -ErrorAction Stop }
    catch { Fail $Case "the ACL cannot be read: $(Get-Acl-Text $Path)"; return }
    $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
    $mine = $rules | Where-Object { $_.IdentityReference -eq $MySid -and $_.AccessControlType -eq 'Allow' }
    if ($rules.Count -ne 1 -or -not $mine -or -not $acl.AreAccessRulesProtected) {
        Fail $Case "not owner-only after repair: $(Get-Acl-Text $Path)"
        return
    }
    Pass $Case "owner-only and protected: $Path"
}

function Test-TokenKept([string]$Case, [string]$Root, [string]$Token) {
    try { $now = (Get-Content (Join-Path $Root '.naht\token') -Raw -ErrorAction Stop).Trim() }
    catch { Fail $Case "the token cannot be read: $_"; return }
    if ($now -ne $Token) { Fail $Case 'the token changed' } else { Pass $Case 'the token is unchanged' }
}

function Test-NoTokenLeak([string]$Case, [string]$Text, [string]$Token) {
    if ($Text.Contains($Token)) { Fail $Case 'the token appears in the output' }
}

New-Item -ItemType Directory -Force -Path $Work | Out-Null
Write-Host "running as $Me ($MySid)"

$case = 'deny-ace-file'
$root = Join-Path $Work $case
$token = New-Project $root
$file = Join-Path $root '.naht\token'
& icacls $file /deny "${Me}:(R)" | Out-Null
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
if (-not $run.Serving) { Fail $case "serve did not start: $($run.Text)" }
elseif ($run.Text -notmatch 'the token file was open to other accounts') { Fail $case "no repair warning: $($run.Text)" }
else { Pass $case 'serve repaired the deny ACE with one warning' }
Test-OwnerOnly $case $file
Test-TokenKept $case $root $token

$case = 'deny-ace-dir'
$root = Join-Path $Work $case
$token = New-Project $root
$dir = Join-Path $root '.naht'
& icacls $dir /deny "${Me}:(OI)(CI)(RD)" | Out-Null
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
if (-not $run.Serving) { Fail $case "serve did not start: $($run.Text)" }
elseif ($run.Text -notmatch 'the state directory was open to other accounts') { Fail $case "no repair warning: $($run.Text)" }
else { Pass $case 'serve repaired the deny ACE on .naht with one warning' }
Test-OwnerOnly $case $dir
Test-TokenKept $case $root $token

$password = 'N' + [guid]::NewGuid().ToString('N') + '!a1'
New-LocalUser -Name $Other -Password (ConvertTo-SecureString $password -AsPlainText -Force) | Out-Null

$case = 'foreign-owner-file'
$root = Join-Path $Work $case
$token = New-Project $root
$file = Join-Path $root '.naht\token'
& icacls $file /setowner $Other | Out-Null
& icacls $file /inheritance:r /grant:r "${Other}:(F)" | Out-Null
& icacls $file /remove:g $Me | Out-Null
Write-Host "ACL before serve: $(Get-Acl-Text $file)"
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
$named = [regex]::Match($run.Text, 'takeown /F "([^"]+)"').Groups[1].Value
if ($run.Serving) { Fail $case "serve started on a file another account owns: $($run.Text)" }
elseif (-not $named.EndsWith('.naht\token')) { Fail $case "the refusal does not name takeown on the token file: $($run.Text)" }
else { Pass $case "serve refused and named takeown /F `"$named`"" }
& takeown /F $named | Out-Null
if ($LASTEXITCODE -ne 0) { Fail $case "the takeown command naht gave failed on `"$named`"" }
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
if (-not $run.Serving) { Fail $case "serve did not start after takeown: $($run.Text)" }
else { Pass $case 'serve started after takeown' }
Test-OwnerOnly $case $file
Test-TokenKept $case $root $token

$case = 'foreign-owner-dir'
$root = Join-Path $Work $case
$token = New-Project $root
$dir = Join-Path $root '.naht'
& icacls $dir /setowner $Other /T /C | Out-Null
& icacls $dir /inheritance:r /grant:r "${Other}:(OI)(CI)(F)" /T /C | Out-Null
& icacls $dir /remove:g $Me /T /C | Out-Null
Write-Host "ACL before serve: $(Get-Acl-Text $dir)"
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
$namedTakeown = [regex]::Match($run.Text, 'takeown /F "([^"]+)" /R /D Y').Groups[1].Value
$namedReset = [regex]::Match($run.Text, 'icacls "([^"]+)" /reset /T /C').Groups[1].Value
if ($run.Serving) { Fail $case "serve started on a directory another account owns: $($run.Text)" }
elseif (-not ($namedTakeown.EndsWith('.naht') -and $namedReset -eq $namedTakeown)) {
    Fail $case "the refusal does not give the takeown and icacls steps on .naht: $($run.Text)"
}
else { Pass $case "serve refused and named takeown and icacls /reset on `"$namedTakeown`"" }
& takeown /F $namedTakeown /R /D Y | Out-Null
if ($LASTEXITCODE -ne 0) { Fail $case "the takeown command naht gave failed on `"$namedTakeown`"" }
& icacls $namedReset /reset /T /C | Out-Null
if ($LASTEXITCODE -ne 0) { Fail $case "the icacls /reset command naht gave failed on `"$namedReset`"" }
$run = Invoke-Serve $root
Test-NoTokenLeak $case $run.Text $token
if (-not $run.Serving) { Fail $case "serve did not start after the steps: $($run.Text)" }
else { Pass $case 'serve started after takeown and icacls /reset' }
Test-OwnerOnly $case $dir
Test-OwnerOnly $case (Join-Path $dir 'token')
Test-TokenKept $case $root $token

Remove-LocalUser -Name $Other

foreach ($fs in @('FAT32', 'exFAT')) {
    $case = "volume-$fs"
    $vhd = Join-Path $Work "$fs.vhdx"
    $letter = if ($fs -eq 'FAT32') { 'R' } else { 'S' }
    $script = Join-Path $Work "$fs.diskpart"
    @(
        "create vdisk file=`"$vhd`" maximum=128 type=expandable",
        "select vdisk file=`"$vhd`"",
        'attach vdisk',
        'create partition primary',
        "format fs=$fs quick label=NAHT",
        "assign letter=$letter"
    ) | Set-Content -Path $script -Encoding ascii
    & diskpart /s $script | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail $case 'diskpart could not make the volume'; continue }
    $root = "${letter}:\project"
    & $Naht init $root | Out-Null
    $run = Invoke-Serve $root
    $tokenFile = Join-Path $root '.naht\token'
    if (-not $run.Serving) { Fail $case "serve did not start on $fs: $($run.Text)" }
    elseif (-not (Test-Path $tokenFile)) { Fail $case "no token file on $fs: $($run.Text)" }
    else { Pass $case "serve started and created the token on $fs, which keeps no permissions" }
    if (Test-Path $tokenFile) { Test-NoTokenLeak $case $run.Text ((Get-Content $tokenFile -Raw).Trim()) }
    @("select vdisk file=`"$vhd`"", 'detach vdisk') | Set-Content -Path $script -Encoding ascii
    & diskpart /s $script | Out-Null
}

if ($script:Failures.Count -gt 0) {
    Write-Host "$($script:Failures.Count) failure(s):"
    $script:Failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host 'all Windows token trial checks passed'
