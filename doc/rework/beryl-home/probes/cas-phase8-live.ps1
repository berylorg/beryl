param(
    [string] $CodexPath = 'C:\Users\user\apps\bin\codex.exe',
    [switch] $KeepArtifacts
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not (Test-Path -LiteralPath $CodexPath -PathType Leaf)) {
    throw "Configured Codex executable does not exist: $CodexPath"
}

$version = (& $CodexPath --version | Out-String).Trim()
if ($version -ne 'codex-cli 0.144.1') {
    throw "Expected codex-cli 0.144.1, observed: $version"
}

$binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $CodexPath).Hash
$probeRoot = Join-Path $env:TEMP ('beryl-cas-phase8-live-' + [guid]::NewGuid().ToString('N'))
$codexHome = Join-Path $probeRoot 'codex-home'
$workRoot = Join-Path $probeRoot 'work'
$captureRoot = Join-Path $probeRoot 'captures'
$schemaRoot = Join-Path $probeRoot 'schema'
foreach ($path in @($probeRoot, $codexHome, $workRoot, $captureRoot, $schemaRoot)) {
    New-Item -ItemType Directory -Path $path | Out-Null
}

function Get-Utf8Sha256 {
    param([Parameter(Mandatory)] [string] $Text)

    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($bytes)
        return ([BitConverter]::ToString($hash)).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

function New-ExactAsciiPayload {
    param(
        [Parameter(Mandatory)] [int] $Length,
        [Parameter(Mandatory)] [string] $Seed
    )

    if ($Length -lt 0 -or [string]::IsNullOrEmpty($Seed)) {
        throw 'Payload length and seed are invalid.'
    }
    $count = [int][Math]::Ceiling($Length / [double]$Seed.Length)
    return (($Seed * $count).Substring(0, $Length))
}

function Get-FreeTcpPort {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try {
        return ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

$port = Get-FreeTcpPort
$readyPath = Join-Path $captureRoot 'ready'
$stopPath = Join-Path $captureRoot 'stop'
$serverJob = Start-Job -ArgumentList $port, $captureRoot, $readyPath, $stopPath -ScriptBlock {
    param($Port, $CaptureRoot, $ReadyPath, $StopPath)

    $ErrorActionPreference = 'Stop'
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, [int]$Port)
    $listener.Start()
    [IO.File]::WriteAllText($ReadyPath, 'ready', [Text.UTF8Encoding]::new($false))
    $requestNumber = 0
    try {
        while (-not (Test-Path -LiteralPath $StopPath)) {
            if (-not $listener.Pending()) {
                Start-Sleep -Milliseconds 25
                continue
            }
            $client = $listener.AcceptTcpClient()
            try {
                $stream = $client.GetStream()
                $headerBytes = [Collections.Generic.List[byte]]::new()
                $window = [Collections.Generic.Queue[byte]]::new()
                while ($true) {
                    $value = $stream.ReadByte()
                    if ($value -lt 0) { throw 'Capture client closed before HTTP headers completed.' }
                    $headerBytes.Add([byte]$value)
                    $window.Enqueue([byte]$value)
                    while ($window.Count -gt 4) { [void]$window.Dequeue() }
                    if ($window.Count -eq 4) {
                        $tail = $window.ToArray()
                        if ($tail[0] -eq 13 -and $tail[1] -eq 10 -and $tail[2] -eq 13 -and $tail[3] -eq 10) { break }
                    }
                }

                $headers = [Text.Encoding]::ASCII.GetString($headerBytes.ToArray())
                $contentLengthMatch = [regex]::Match($headers, '(?im)^Content-Length:\s*(\d+)\s*$')
                if (-not $contentLengthMatch.Success) { throw 'Capture request did not provide Content-Length.' }
                $contentLength = [int]$contentLengthMatch.Groups[1].Value
                $bodyBytes = [byte[]]::new($contentLength)
                $offset = 0
                while ($offset -lt $contentLength) {
                    $read = $stream.Read($bodyBytes, $offset, $contentLength - $offset)
                    if ($read -le 0) { throw 'Capture client closed before request body completed.' }
                    $offset += $read
                }

                $requestNumber++
                $body = [Text.Encoding]::UTF8.GetString($bodyBytes)
                $capturePath = Join-Path $CaptureRoot ('request-{0:D3}.json' -f $requestNumber)
                [IO.File]::WriteAllBytes($capturePath, $bodyBytes)

                $responseId = 'resp-probe-{0:D3}' -f $requestNumber
                $assistantText = 'MOCK_ASSISTANT_{0:D3}' -f $requestNumber
                $events = @(
                    @{ type = 'response.created'; response = @{ id = $responseId } },
                    @{ type = 'response.output_item.done'; item = @{ type = 'message'; role = 'assistant'; id = ('msg-probe-{0:D3}' -f $requestNumber); content = @(@{ type = 'output_text'; text = $assistantText }) } },
                    @{ type = 'response.completed'; response = @{ id = $responseId; usage = @{ input_tokens = 0; input_tokens_details = $null; output_tokens = 0; output_tokens_details = $null; total_tokens = 0 } } }
                )
                $sse = (($events | ForEach-Object {
                    $json = ConvertTo-Json -InputObject $_ -Depth 20 -Compress
                    "event: $($_.type)`ndata: $json`n`n"
                }) -join '')
                $responseBytes = [Text.Encoding]::UTF8.GetBytes($sse)
                $responseHeaders = "HTTP/1.1 200 OK`r`nContent-Type: text/event-stream`r`nContent-Length: $($responseBytes.Length)`r`nConnection: close`r`n`r`n"
                $headerResponseBytes = [Text.Encoding]::ASCII.GetBytes($responseHeaders)
                $stream.Write($headerResponseBytes, 0, $headerResponseBytes.Length)
                $stream.Write($responseBytes, 0, $responseBytes.Length)
                $stream.Flush()
            }
            finally {
                $client.Dispose()
            }
        }
    }
    finally {
        $listener.Stop()
    }
}

$readyDeadline = [DateTime]::UtcNow.AddSeconds(15)
while (-not (Test-Path -LiteralPath $readyPath)) {
    if ([DateTime]::UtcNow -ge $readyDeadline) { throw 'Timed out waiting for local capture provider.' }
    Start-Sleep -Milliseconds 50
}

$config = @"
model = "gpt-5.4"
model_provider = "beryl_probe"
model_context_window = 1000000
model_auto_compact_token_limit = 900000

[analytics]
enabled = false

[model_providers.beryl_probe]
name = "Beryl Phase 8 Capture Provider"
base_url = "http://127.0.0.1:$port/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
stream_idle_timeout_ms = 30000
"@
[IO.File]::WriteAllText((Join-Path $codexHome 'config.toml'), $config, [Text.UTF8Encoding]::new($false))

& $CodexPath app-server generate-json-schema --out $schemaRoot
if ($LASTEXITCODE -ne 0) { throw 'Stable app-server schema generation failed.' }
$injectSchemaPath = Join-Path $schemaRoot 'v2\ThreadInjectItemsParams.json'
if (-not (Test-Path -LiteralPath $injectSchemaPath -PathType Leaf)) {
    throw 'Stable schema does not expose ThreadInjectItemsParams.'
}

$script:ProtocolMessages = [Collections.Generic.List[object]]::new()

function Start-CasServer {
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $CodexPath
    $psi.Arguments = 'app-server --stdio --strict-config'
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = $workRoot
    $psi.EnvironmentVariables['CODEX_HOME'] = $codexHome

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (-not $process.Start()) { throw 'Failed to start Codex App Server.' }
    return [pscustomobject]@{
        Process = $process
        StderrTask = $process.StandardError.ReadToEndAsync()
    }
}

function Stop-CasServer {
    param([Parameter(Mandatory)] [object] $Server)

    $process = $Server.Process
    if (-not $process.HasExited) {
        $process.StandardInput.Close()
        if (-not $process.WaitForExit(5000)) {
            $process.Kill()
            $process.WaitForExit()
        }
    }
    $stderr = $Server.StderrTask.Result
    if (-not [string]::IsNullOrWhiteSpace($stderr)) {
        [IO.File]::AppendAllText((Join-Path $probeRoot 'app-server.stderr.log'), $stderr, [Text.UTF8Encoding]::new($false))
    }
}

function Send-CasMessage {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [object] $Message
    )

    $json = ConvertTo-Json -InputObject $Message -Depth 50 -Compress
    $Server.Process.StandardInput.WriteLine($json)
    $Server.Process.StandardInput.Flush()
}

function Read-CasMessage {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [int] $TimeoutMs = 60000
    )

    $task = $Server.Process.StandardOutput.ReadLineAsync()
    if (-not $task.Wait($TimeoutMs)) { throw "Timed out after $TimeoutMs ms waiting for app-server output." }
    $line = $task.Result
    if ($null -eq $line) { throw 'App-server stdout closed unexpectedly.' }
    $message = $line | ConvertFrom-Json
    $script:ProtocolMessages.Add($message)
    return $message
}

function Wait-CasResponse {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [int] $Id,
        [switch] $AllowError
    )

    while ($true) {
        $message = Read-CasMessage -Server $Server
        $idProperty = $message.PSObject.Properties['id']
        if ($null -ne $idProperty -and [int]$idProperty.Value -eq $Id) {
            $errorProperty = $message.PSObject.Properties['error']
            if ($null -ne $errorProperty -and $null -ne $errorProperty.Value -and -not $AllowError) {
                throw ('CAS request failed: ' + ($errorProperty.Value | ConvertTo-Json -Depth 20 -Compress))
            }
            return $message
        }
    }
}

function Wait-TurnCompletion {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [Parameter(Mandatory)] [string] $TurnId
    )

    while ($true) {
        $message = Read-CasMessage -Server $Server
        $methodProperty = $message.PSObject.Properties['method']
        if ($null -ne $methodProperty -and $methodProperty.Value -eq 'turn/completed' -and
            $message.params.threadId -eq $ThreadId -and
            $message.params.turn.id -eq $TurnId) {
            if ($message.params.turn.status -ne 'completed') {
                throw "Turn $TurnId completed with status $($message.params.turn.status)."
            }
            return $message
        }
    }
}

function Initialize-CasServer {
    param([Parameter(Mandatory)] [object] $Server)

    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'initialize'
        id = 0
        params = [ordered]@{
            clientInfo = [ordered]@{
                name = 'beryl_phase8_live_probe'
                title = 'Beryl Phase 8 Live Probe'
                version = '0.1.0'
            }
            capabilities = [ordered]@{ experimentalApi = $false }
        }
    })
    $response = Wait-CasResponse -Server $Server -Id 0
    Send-CasMessage -Server $Server -Message ([ordered]@{ method = 'initialized'; params = @{} })
    return [string]$response.result.userAgent
}

$script:NextRequestId = 1

function New-CasRequestId {
    $id = $script:NextRequestId
    $script:NextRequestId++
    return $id
}

function Start-ProbeThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [bool] $Ephemeral = $false,
        [string] $DeveloperInstructions = 'Beryl Phase 8 deterministic capture probe. Do not call tools.'
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'thread/start'
        id = $id
        params = [ordered]@{
            cwd = $workRoot
            model = 'gpt-5.4'
            modelProvider = 'beryl_probe'
            developerInstructions = $DeveloperInstructions
            ephemeral = $Ephemeral
            approvalPolicy = 'never'
            sandbox = 'read-only'
        }
    })
    $response = Wait-CasResponse -Server $Server -Id $id
    return [string]$response.result.thread.id
}

function Resume-ProbeThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'thread/resume'
        id = $id
        params = [ordered]@{
            threadId = $ThreadId
            cwd = $workRoot
            model = 'gpt-5.4'
            modelProvider = 'beryl_probe'
            developerInstructions = 'Beryl Phase 8 deterministic capture probe. Do not call tools.'
            approvalPolicy = 'never'
            sandbox = 'read-only'
        }
    })
    $response = Wait-CasResponse -Server $Server -Id $id
    if ([string]$response.result.thread.id -ne $ThreadId) { throw 'thread/resume changed thread identity.' }
}

function Fork-ProbeThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [string] $LastTurnId = $null
    )

    $params = [ordered]@{
        threadId = $ThreadId
        cwd = $workRoot
        model = 'gpt-5.4'
        modelProvider = 'beryl_probe'
        developerInstructions = 'Beryl Phase 8 deterministic capture probe. Do not call tools.'
        ephemeral = $false
        approvalPolicy = 'never'
        sandbox = 'read-only'
    }
    if (-not [string]::IsNullOrEmpty($LastTurnId)) { $params['lastTurnId'] = $LastTurnId }
    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{ method = 'thread/fork'; id = $id; params = $params })
    $response = Wait-CasResponse -Server $Server -Id $id
    return [string]$response.result.thread.id
}

function Rollback-ProbeThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [Parameter(Mandatory)] [int] $NumTurns
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'thread/rollback'
        id = $id
        params = [ordered]@{ threadId = $ThreadId; numTurns = $NumTurns }
    })
    [void](Wait-CasResponse -Server $Server -Id $id)
}

function Start-ProbeTurn {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [Parameter(Mandatory)] [string] $Text
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'turn/start'
        id = $id
        params = [ordered]@{
            threadId = $ThreadId
            input = @([ordered]@{ type = 'text'; text = $Text })
        }
    })
    $response = Wait-CasResponse -Server $Server -Id $id
    $turnId = [string]$response.result.turn.id
    [void](Wait-TurnCompletion -Server $Server -ThreadId $ThreadId -TurnId $turnId)
    return $turnId
}

function Inject-ProbeItems {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [Parameter(Mandatory)] [object[]] $Items,
        [switch] $AllowError
    )

    $beforeTurnStarts = @($script:ProtocolMessages | Where-Object {
        $property = $_.PSObject.Properties['method']
        $null -ne $property -and $property.Value -eq 'turn/started'
    }).Count
    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'thread/inject_items'
        id = $id
        params = [ordered]@{ threadId = $ThreadId; items = $Items }
    })
    $response = Wait-CasResponse -Server $Server -Id $id -AllowError:$AllowError
    $afterTurnStarts = @($script:ProtocolMessages | Where-Object {
        $property = $_.PSObject.Properties['method']
        $null -ne $property -and $property.Value -eq 'turn/started'
    }).Count
    if ($afterTurnStarts -ne $beforeTurnStarts) { throw 'thread/inject_items emitted an ordinary turn/started lifecycle.' }
    return $response
}

function Get-CaptureFiles {
    return @(Get-ChildItem -LiteralPath $captureRoot -Filter 'request-*.json' -File | Sort-Object Name)
}

function Wait-CaptureCount {
    param([Parameter(Mandatory)] [int] $Count)

    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (@(Get-CaptureFiles).Count -lt $Count) {
        if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for capture count $Count." }
        Start-Sleep -Milliseconds 50
    }
}

function Read-CapturedRequest {
    param([Parameter(Mandatory)] [int] $Number)

    Wait-CaptureCount -Count $Number
    $path = Join-Path $captureRoot ('request-{0:D3}.json' -f $Number)
    $raw = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8)
    return [pscustomobject]@{
        Number = $Number
        Path = $path
        Raw = $raw
        Json = ($raw | ConvertFrom-Json)
    }
}

function Get-CapturedTextRecords {
    param([Parameter(Mandatory)] [object] $Capture)

    $records = [Collections.Generic.List[object]]::new()
    $index = 0
    foreach ($item in @($Capture.Json.input)) {
        $roleProperty = $item.PSObject.Properties['role']
        $role = if ($null -ne $roleProperty) { [string]$roleProperty.Value } else { '' }
        $contentProperty = $item.PSObject.Properties['content']
        if ($null -eq $contentProperty) { $index++; continue }
        foreach ($content in @($contentProperty.Value)) {
            $textProperty = $content.PSObject.Properties['text']
            if ($null -ne $textProperty) {
                $records.Add([pscustomobject]@{
                    Index = $index
                    Role = $role
                    Type = [string]$content.type
                    Text = [string]$textProperty.Value
                })
            }
        }
        $index++
    }
    return $records.ToArray()
}

function Find-TextRecord {
    param(
        [Parameter(Mandatory)] [object] $Capture,
        [Parameter(Mandatory)] [string] $Marker
    )

    $matches = @(Get-CapturedTextRecords -Capture $Capture | Where-Object { $_.Text.Contains($Marker) })
    if ($matches.Count -ne 1) { throw "Expected exactly one captured text record containing '$Marker'; observed $($matches.Count)." }
    return $matches[0]
}

function Get-RawMarkerCount {
    param(
        [Parameter(Mandatory)] [object] $Capture,
        [Parameter(Mandatory)] [string] $Marker
    )

    return ([regex]::Matches($Capture.Raw, [regex]::Escape($Marker))).Count
}

$script:Checks = [Collections.Generic.List[object]]::new()

function Add-Check {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [bool] $Passed,
        [Parameter(Mandatory)] [string] $Detail
    )

    $script:Checks.Add([pscustomobject]@{ name = $Name; passed = $Passed; detail = $Detail })
    if (-not $Passed) { throw "Proof check failed: $Name - $Detail" }
}

function Assert-MarkerCount {
    param(
        [Parameter(Mandatory)] [object] $Capture,
        [Parameter(Mandatory)] [string] $Marker,
        [Parameter(Mandatory)] [int] $Expected,
        [Parameter(Mandatory)] [string] $Name
    )

    $actual = Get-RawMarkerCount -Capture $Capture -Marker $Marker
    Add-Check -Name $Name -Passed ($actual -eq $Expected) -Detail "marker=$Marker expected=$Expected actual=$actual capture=$($Capture.Number)"
}

function Assert-TextRecordExact {
    param(
        [Parameter(Mandatory)] [object] $Capture,
        [Parameter(Mandatory)] [string] $Marker,
        [Parameter(Mandatory)] [string] $ExpectedRole,
        [Parameter(Mandatory)] [string] $ExpectedText,
        [Parameter(Mandatory)] [string] $Name
    )

    $record = Find-TextRecord -Capture $Capture -Marker $Marker
    $actualBytes = [Text.Encoding]::UTF8.GetByteCount($record.Text)
    $expectedBytes = [Text.Encoding]::UTF8.GetByteCount($ExpectedText)
    $passed = $record.Role -eq $ExpectedRole -and $record.Text -ceq $ExpectedText
    $detail = "role=$($record.Role) expected_role=$ExpectedRole bytes=$actualBytes expected_bytes=$expectedBytes sha256=$(Get-Utf8Sha256 -Text $record.Text)"
    Add-Check -Name $Name -Passed $passed -Detail $detail
}

function Read-PublicThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'thread/read'
        id = $id
        params = [ordered]@{ threadId = $ThreadId; includeTurns = $true }
    })
    return (Wait-CasResponse -Server $Server -Id $id)
}

$nativeUser1 = 'NATIVE_USER_ONE_1A0F66D2'
$nativeUser2 = 'NATIVE_USER_TWO_8ED02341'
$nativeUser3 = 'NATIVE_USER_THREE_90E4B53C'
$forkUser = 'NATIVE_FORK_USER_A70B2C19'
$rollbackUser = 'NATIVE_ROLLBACK_USER_B19F036D'
$recoveryUser = 'RECOVERY_INJECTED_USER_F170C2A5'
$recoveryAssistant = 'RECOVERY_INJECTED_ASSISTANT_6420DB8E'
$recoveryRealUser = 'RECOVERY_REAL_USER_C594A2F7'
$recoveryResumeUser = 'RECOVERY_RESUME_USER_4A23D8C1'
$server = $null

try {
    $server = Start-CasServer
    $userAgent1 = Initialize-CasServer -Server $server
    Add-Check -Name 'initialize-target-version' -Passed ($userAgent1.Contains('0.144.1')) -Detail "user_agent=$userAgent1"

    $nativeThread = Start-ProbeThread -Server $server
    $nativeTurn1 = Start-ProbeTurn -Server $server -ThreadId $nativeThread -Text $nativeUser1
    Add-Check -Name 'native-first-turn-id-returned' -Passed (-not [string]::IsNullOrWhiteSpace([string]$nativeTurn1)) -Detail "turn_id=$nativeTurn1"
    $capture1 = Read-CapturedRequest -Number 1
    Assert-MarkerCount -Capture $capture1 -Marker $nativeUser1 -Expected 1 -Name 'native-first-turn-current-input-once'

    $nativeTurn2 = Start-ProbeTurn -Server $server -ThreadId $nativeThread -Text $nativeUser2
    Add-Check -Name 'native-second-turn-id-returned' -Passed (-not [string]::IsNullOrWhiteSpace([string]$nativeTurn2)) -Detail "turn_id=$nativeTurn2"
    $capture2 = Read-CapturedRequest -Number 2
    Assert-MarkerCount -Capture $capture2 -Marker $nativeUser1 -Expected 1 -Name 'native-continuation-inherits-first-user-once'
    Assert-MarkerCount -Capture $capture2 -Marker 'MOCK_ASSISTANT_001' -Expected 1 -Name 'native-continuation-inherits-first-assistant-once'
    Assert-MarkerCount -Capture $capture2 -Marker $nativeUser2 -Expected 1 -Name 'native-continuation-current-user-once'

    Stop-CasServer -Server $server
    $server = Start-CasServer
    $userAgent2 = Initialize-CasServer -Server $server
    Add-Check -Name 'resume-target-version' -Passed ($userAgent2.Contains('0.144.1')) -Detail "user_agent=$userAgent2"
    Resume-ProbeThread -Server $server -ThreadId $nativeThread

    $nativeTurn3 = Start-ProbeTurn -Server $server -ThreadId $nativeThread -Text $nativeUser3
    Add-Check -Name 'native-third-turn-id-returned' -Passed (-not [string]::IsNullOrWhiteSpace([string]$nativeTurn3)) -Detail "turn_id=$nativeTurn3"
    $capture3 = Read-CapturedRequest -Number 3
    foreach ($entry in @(
        [pscustomobject]@{ marker = $nativeUser1; name = 'native-resume-first-user-once' },
        [pscustomobject]@{ marker = 'MOCK_ASSISTANT_001'; name = 'native-resume-first-assistant-once' },
        [pscustomobject]@{ marker = $nativeUser2; name = 'native-resume-second-user-once' },
        [pscustomobject]@{ marker = 'MOCK_ASSISTANT_002'; name = 'native-resume-second-assistant-once' },
        [pscustomobject]@{ marker = $nativeUser3; name = 'native-resume-current-user-once' }
    )) {
        Assert-MarkerCount -Capture $capture3 -Marker $entry.marker -Expected 1 -Name $entry.name
    }

    $forkThread = Fork-ProbeThread -Server $server -ThreadId $nativeThread -LastTurnId $nativeTurn1
    [void](Start-ProbeTurn -Server $server -ThreadId $forkThread -Text $forkUser)
    $capture4 = Read-CapturedRequest -Number 4
    Assert-MarkerCount -Capture $capture4 -Marker $nativeUser1 -Expected 1 -Name 'native-fork-inherits-boundary-user'
    Assert-MarkerCount -Capture $capture4 -Marker 'MOCK_ASSISTANT_001' -Expected 1 -Name 'native-fork-inherits-boundary-assistant'
    Assert-MarkerCount -Capture $capture4 -Marker $nativeUser2 -Expected 0 -Name 'native-fork-excludes-later-user'
    Assert-MarkerCount -Capture $capture4 -Marker 'MOCK_ASSISTANT_002' -Expected 0 -Name 'native-fork-excludes-later-assistant'
    Assert-MarkerCount -Capture $capture4 -Marker $nativeUser3 -Expected 0 -Name 'native-fork-excludes-resume-turn'
    Assert-MarkerCount -Capture $capture4 -Marker $forkUser -Expected 1 -Name 'native-fork-current-user-once'

    Rollback-ProbeThread -Server $server -ThreadId $nativeThread -NumTurns 2
    [void](Start-ProbeTurn -Server $server -ThreadId $nativeThread -Text $rollbackUser)
    $capture5 = Read-CapturedRequest -Number 5
    Assert-MarkerCount -Capture $capture5 -Marker $nativeUser1 -Expected 1 -Name 'native-rollback-retains-first-user'
    Assert-MarkerCount -Capture $capture5 -Marker 'MOCK_ASSISTANT_001' -Expected 1 -Name 'native-rollback-retains-first-assistant'
    Assert-MarkerCount -Capture $capture5 -Marker $nativeUser2 -Expected 0 -Name 'native-rollback-removes-second-user'
    Assert-MarkerCount -Capture $capture5 -Marker $nativeUser3 -Expected 0 -Name 'native-rollback-removes-third-user'
    Assert-MarkerCount -Capture $capture5 -Marker $rollbackUser -Expected 1 -Name 'native-rollback-current-user-once'

    $recoveryThread = Start-ProbeThread -Server $server
    $recoveryItems = @(
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $recoveryUser }) },
        [ordered]@{ type = 'message'; role = 'assistant'; content = @([ordered]@{ type = 'output_text'; text = $recoveryAssistant }) }
    )
    [void](Inject-ProbeItems -Server $server -ThreadId $recoveryThread -Items $recoveryItems)
    [void](Start-ProbeTurn -Server $server -ThreadId $recoveryThread -Text $recoveryRealUser)
    $capture6 = Read-CapturedRequest -Number 6
    Assert-TextRecordExact -Capture $capture6 -Marker $recoveryUser -ExpectedRole 'user' -ExpectedText $recoveryUser -Name 'recovery-user-role-and-text-exact'
    Assert-TextRecordExact -Capture $capture6 -Marker $recoveryAssistant -ExpectedRole 'assistant' -ExpectedText $recoveryAssistant -Name 'recovery-assistant-role-and-text-exact'
    Assert-MarkerCount -Capture $capture6 -Marker $recoveryRealUser -Expected 1 -Name 'recovery-real-user-follows-prefix-once'

    $publicRecovery = Read-PublicThread -Server $server -ThreadId $recoveryThread
    $publicRecoveryJson = ConvertTo-Json -InputObject $publicRecovery.result -Depth 50 -Compress
    Add-Check -Name 'recovery-injection-not-public-turn' -Passed (-not $publicRecoveryJson.Contains($recoveryUser) -and -not $publicRecoveryJson.Contains($recoveryAssistant)) -Detail 'thread/read omitted ordinary injected raw messages'

    Stop-CasServer -Server $server
    $server = Start-CasServer
    [void](Initialize-CasServer -Server $server)
    Resume-ProbeThread -Server $server -ThreadId $recoveryThread
    [void](Start-ProbeTurn -Server $server -ThreadId $recoveryThread -Text $recoveryResumeUser)
    $capture7 = Read-CapturedRequest -Number 7
    Assert-MarkerCount -Capture $capture7 -Marker $recoveryUser -Expected 1 -Name 'recovery-normal-resume-user-prefix-once'
    Assert-MarkerCount -Capture $capture7 -Marker $recoveryAssistant -Expected 1 -Name 'recovery-normal-resume-assistant-prefix-once'
    Assert-MarkerCount -Capture $capture7 -Marker $recoveryRealUser -Expected 1 -Name 'recovery-normal-resume-first-real-user-once'
    Assert-MarkerCount -Capture $capture7 -Marker $recoveryResumeUser -Expected 1 -Name 'recovery-normal-resume-current-user-once'

    $recoveryForkUser = 'RECOVERY_FULL_FORK_USER_94BC036A'
    $recoveryFork = Fork-ProbeThread -Server $server -ThreadId $recoveryThread
    [void](Start-ProbeTurn -Server $server -ThreadId $recoveryFork -Text $recoveryForkUser)
    $capture8 = Read-CapturedRequest -Number 8
    Assert-MarkerCount -Capture $capture8 -Marker $recoveryUser -Expected 1 -Name 'recovery-full-fork-user-prefix-once'
    Assert-MarkerCount -Capture $capture8 -Marker $recoveryAssistant -Expected 1 -Name 'recovery-full-fork-assistant-prefix-once'
    Assert-MarkerCount -Capture $capture8 -Marker $recoveryRealUser -Expected 1 -Name 'recovery-full-fork-real-user-once'
    Assert-MarkerCount -Capture $capture8 -Marker $recoveryResumeUser -Expected 1 -Name 'recovery-full-fork-resume-user-once'
    Assert-MarkerCount -Capture $capture8 -Marker $recoveryForkUser -Expected 1 -Name 'recovery-full-fork-current-user-once'

    $branchSelection = New-ExactAsciiPayload -Length 65536 -Seed 'BRANCH_SELECTED_ASSISTANT_PASSAGE_C54E190B_'
    $branchSelectionHash = Get-Utf8Sha256 -Text $branchSelection
    $branchPrefix = "[Beryl branch discussion context v1]`nsource_role=assistant`nselected_utf8_bytes=65536`nselected_sha256=$branchSelectionHash`n`n"
    $branchContext = $branchPrefix + $branchSelection
    $branchUser = 'BRANCH_FIRST_REAL_USER_70DE218F'
    $branchThread = Fork-ProbeThread -Server $server -ThreadId $nativeThread -LastTurnId $nativeTurn1
    $branchItem = [ordered]@{
        type = 'message'
        role = 'assistant'
        content = @([ordered]@{ type = 'output_text'; text = $branchContext })
    }
    [void](Inject-ProbeItems -Server $server -ThreadId $branchThread -Items @($branchItem))
    [void](Start-ProbeTurn -Server $server -ThreadId $branchThread -Text $branchUser)
    $capture9 = Read-CapturedRequest -Number 9
    Assert-TextRecordExact -Capture $capture9 -Marker '[Beryl branch discussion context v1]' -ExpectedRole 'assistant' -ExpectedText $branchContext -Name 'branch-selection-assistant-provenance-exact'
    Add-Check -Name 'branch-selection-exact-bound' -Passed ([Text.Encoding]::UTF8.GetByteCount($branchSelection) -eq 65536) -Detail "selected_bytes=65536 sha256=$branchSelectionHash framed_bytes=$([Text.Encoding]::UTF8.GetByteCount($branchContext))"
    Assert-MarkerCount -Capture $capture9 -Marker $branchUser -Expected 1 -Name 'branch-real-user-follows-context-once'
    $publicBranch = Read-PublicThread -Server $server -ThreadId $branchThread
    $publicBranchJson = ConvertTo-Json -InputObject $publicBranch.result -Depth 50 -Compress
    Add-Check -Name 'branch-context-not-public-turn' -Passed (-not $publicBranchJson.Contains('BRANCH_SELECTED_ASSISTANT_PASSAGE_C54E190B_')) -Detail 'thread/read omitted synthetic branch context'

    $boundTexts = @(
        (New-ExactAsciiPayload -Length 65536 -Seed 'BOUND_USER_ONE_01D6C38A_'),
        (New-ExactAsciiPayload -Length 65536 -Seed 'BOUND_ASSISTANT_ONE_67F0B4D2_'),
        (New-ExactAsciiPayload -Length 65536 -Seed 'BOUND_USER_TWO_A1972E4C_'),
        (New-ExactAsciiPayload -Length 65536 -Seed 'BOUND_ASSISTANT_TWO_39CB50F6_')
    )
    $boundItems = @(
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $boundTexts[0] }) },
        [ordered]@{ type = 'message'; role = 'assistant'; content = @([ordered]@{ type = 'output_text'; text = $boundTexts[1] }) },
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $boundTexts[2] }) },
        [ordered]@{ type = 'message'; role = 'assistant'; content = @([ordered]@{ type = 'output_text'; text = $boundTexts[3] }) }
    )
    $boundThread = Start-ProbeThread -Server $server
    [void](Inject-ProbeItems -Server $server -ThreadId $boundThread -Items $boundItems)
    [void](Start-ProbeTurn -Server $server -ThreadId $boundThread -Text 'BOUND_REAL_USER_2F6A80C1')
    $capture10 = Read-CapturedRequest -Number 10
    $boundRoles = @('user', 'assistant', 'user', 'assistant')
    $boundTotal = 0
    for ($i = 0; $i -lt $boundTexts.Count; $i++) {
        $boundTotal += [Text.Encoding]::UTF8.GetByteCount($boundTexts[$i])
        $marker = $boundTexts[$i].Substring(0, 24)
        Assert-TextRecordExact -Capture $capture10 -Marker $marker -ExpectedRole $boundRoles[$i] -ExpectedText $boundTexts[$i] -Name "recovery-bound-item-$($i + 1)-exact"
    }
    Add-Check -Name 'recovery-projection-transport-ceiling-exact' -Passed ($boundTotal -eq 262144) -Detail "message_text_bytes=$boundTotal request_bytes=$([Text.Encoding]::UTF8.GetByteCount($capture10.Raw))"

    $invalidMarker = 'INVALID_BATCH_MUST_NOT_APPLY_02A1D7C9'
    $remoteMarker = 'REMOTE_IMAGE_BATCH_MUST_NOT_APPLY_BC647FA0'
    $invalidThread = Start-ProbeThread -Server $server
    $invalidResponse = Inject-ProbeItems -Server $server -ThreadId $invalidThread -AllowError -Items @(
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $invalidMarker }) },
        [ordered]@{ type = 'message'; role = 'user'; content = 'not-an-array' }
    )
    Add-Check -Name 'malformed-mixed-batch-rejected' -Passed ($null -ne $invalidResponse.PSObject.Properties['error']) -Detail 'app-server returned an error for malformed item 2'

    $remoteResponse = Inject-ProbeItems -Server $server -ThreadId $invalidThread -AllowError -Items @(
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $remoteMarker }) },
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_image'; image_url = 'https://example.invalid/not-loaded.png' }) }
    )
    Add-Check -Name 'remote-image-mixed-batch-rejected' -Passed ($null -ne $remoteResponse.PSObject.Properties['error']) -Detail 'app-server returned an error for a remote image item'

    [void](Start-ProbeTurn -Server $server -ThreadId $invalidThread -Text 'INVALID_BATCH_REAL_USER_44D3098E')
    $capture11 = Read-CapturedRequest -Number 11
    Assert-MarkerCount -Capture $capture11 -Marker $invalidMarker -Expected 0 -Name 'malformed-batch-had-no-partial-application'
    Assert-MarkerCount -Capture $capture11 -Marker $remoteMarker -Expected 0 -Name 'remote-image-batch-had-no-partial-application'

    $ambiguousOldMarker = 'AMBIGUOUS_ABANDONED_PREFIX_AE7C1540'
    $ambiguousNewMarker = 'AMBIGUOUS_REPLACEMENT_PREFIX_16B94DC2'
    $ambiguousOldThread = Start-ProbeThread -Server $server
    $ambiguousRequestId = New-CasRequestId
    Send-CasMessage -Server $server -Message ([ordered]@{
        method = 'thread/inject_items'
        id = $ambiguousRequestId
        params = [ordered]@{
            threadId = $ambiguousOldThread
            items = @([ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $ambiguousOldMarker }) })
        }
    })
    $discardedResponse = Wait-CasResponse -Server $server -Id $ambiguousRequestId
    Add-Check -Name 'ambiguity-interposer-discarded-target-response' -Passed ($discardedResponse.id -eq $ambiguousRequestId) -Detail "discarded_response_id=$ambiguousRequestId"
    Stop-CasServer -Server $server

    $server = Start-CasServer
    [void](Initialize-CasServer -Server $server)
    $ambiguousNewThread = Start-ProbeThread -Server $server
    Add-Check -Name 'ambiguous-thread-abandoned-for-distinct-thread' -Passed ($ambiguousNewThread -ne $ambiguousOldThread) -Detail "abandoned=$ambiguousOldThread replacement=$ambiguousNewThread"
    [void](Inject-ProbeItems -Server $server -ThreadId $ambiguousNewThread -Items @(
        [ordered]@{ type = 'message'; role = 'user'; content = @([ordered]@{ type = 'input_text'; text = $ambiguousNewMarker }) }
    ))
    [void](Start-ProbeTurn -Server $server -ThreadId $ambiguousNewThread -Text 'AMBIGUOUS_REPLACEMENT_REAL_USER_E02C6B39')
    $capture12 = Read-CapturedRequest -Number 12
    Assert-MarkerCount -Capture $capture12 -Marker $ambiguousOldMarker -Expected 0 -Name 'ambiguous-old-prefix-not-retried-into-replacement'
    Assert-MarkerCount -Capture $capture12 -Marker $ambiguousNewMarker -Expected 1 -Name 'ambiguous-replacement-prefix-injected-once'

    $captureEvidence = @(Get-CaptureFiles | ForEach-Object {
        [pscustomobject]@{
            name = $_.Name
            bytes = $_.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash
        }
    })
    $report = [ordered]@{
        generated_at_utc = [DateTime]::UtcNow.ToString('o')
        codex_version = $version
        codex_sha256 = $binaryHash
        probe_root = $probeRoot
        schema_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $injectSchemaPath).Hash
        captures = $captureEvidence
        checks = $script:Checks.ToArray()
        provider_boundary = 'Local provider proves CAS request construction and transport acceptance, not independent hosted-model context-budget admission.'
    }
    $reportPath = Join-Path $probeRoot 'phase8-report.json'
    $reportJson = ConvertTo-Json -InputObject $report -Depth 20
    [IO.File]::WriteAllText($reportPath, $reportJson, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $probeRoot 'protocol-messages.json'), (ConvertTo-Json -InputObject $script:ProtocolMessages.ToArray() -Depth 50), [Text.UTF8Encoding]::new($false))
    Write-Output $reportJson
}
finally {
    [IO.File]::WriteAllText((Join-Path $probeRoot 'protocol-messages.partial.json'), (ConvertTo-Json -InputObject $script:ProtocolMessages.ToArray() -Depth 50), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $probeRoot 'checks.partial.json'), (ConvertTo-Json -InputObject $script:Checks.ToArray() -Depth 10), [Text.UTF8Encoding]::new($false))
    if ($null -ne $server) {
        Stop-CasServer -Server $server
    }
    if ($null -ne $serverJob) {
        [IO.File]::WriteAllText($stopPath, 'stop', [Text.UTF8Encoding]::new($false))
        [void](Wait-Job -Job $serverJob -Timeout 5 -ErrorAction SilentlyContinue)
        if ($serverJob.State -notin @('Completed', 'Failed', 'Stopped')) {
            Stop-Job -Job $serverJob -ErrorAction SilentlyContinue
        }
        Receive-Job -Job $serverJob -ErrorAction SilentlyContinue | Out-Null
        Remove-Job -Job $serverJob -Force -ErrorAction SilentlyContinue
    }
    if (-not $KeepArtifacts -and $script:Checks.Count -gt 0 -and -not ($script:Checks | Where-Object { -not $_.passed })) {
        Remove-Item -LiteralPath $probeRoot -Recurse -Force
    }
}
