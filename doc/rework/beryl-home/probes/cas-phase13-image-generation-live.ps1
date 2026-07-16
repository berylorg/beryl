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
$probeRoot = Join-Path $env:TEMP ('beryl-cas-phase13-image-generation-live-' + [guid]::NewGuid().ToString('N'))
$codexHome = Join-Path $probeRoot 'codex-home'
$workRoot = Join-Path $probeRoot 'work'
$captureRoot = Join-Path $probeRoot 'captures'
foreach ($path in @($probeRoot, $codexHome, $workRoot, $captureRoot)) {
    New-Item -ItemType Directory -Path $path | Out-Null
}

function Get-Utf8Sha256 {
    param([Parameter(Mandatory)] [string] $Text)

    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
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
$serverJob = $null
$server = $null
$reportPath = Join-Path $probeRoot 'phase13-image-generation-report.json'
$script:ProtocolRecords = [Collections.Generic.List[object]]::new()
$script:Checks = [Collections.Generic.List[object]]::new()
$script:NextRequestId = 1

try {
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
                    if ($value -lt 0) { throw 'Provider client closed before HTTP headers completed.' }
                    $headerBytes.Add([byte]$value)
                    $window.Enqueue([byte]$value)
                    while ($window.Count -gt 4) { [void]$window.Dequeue() }
                    if ($window.Count -eq 4) {
                        $tail = $window.ToArray()
                        if ($tail[0] -eq 13 -and $tail[1] -eq 10 -and $tail[2] -eq 13 -and $tail[3] -eq 10) {
                            break
                        }
                    }
                }

                $headers = [Text.Encoding]::ASCII.GetString($headerBytes.ToArray())
                $headerLines = @($headers -split "`r`n" | Where-Object { -not [string]::IsNullOrEmpty($_) })
                $requestLine = [string]$headerLines[0]
                $headerNames = @($headerLines | Select-Object -Skip 1 | ForEach-Object {
                    $separator = $_.IndexOf(':')
                    if ($separator -gt 0) { $_.Substring(0, $separator).Trim().ToLowerInvariant() }
                })
                $sensitiveHeaderNames = @('authorization', 'proxy-authorization', 'x-api-key')
                $sensitiveHeadersPresent = @($headerNames | Where-Object { $_ -in $sensitiveHeaderNames })

                $contentLengthMatch = [regex]::Match($headers, '(?im)^Content-Length:\s*(\d+)\s*$')
                if (-not $contentLengthMatch.Success) { throw 'Provider request did not provide Content-Length.' }
                $contentLength = [int]$contentLengthMatch.Groups[1].Value
                $bodyBytes = [byte[]]::new($contentLength)
                $offset = 0
                while ($offset -lt $contentLength) {
                    $read = $stream.Read($bodyBytes, $offset, $contentLength - $offset)
                    if ($read -le 0) { throw 'Provider client closed before request body completed.' }
                    $offset += $read
                }

                $requestNumber++
                $body = [Text.Encoding]::UTF8.GetString($bodyBytes)
                $requestPath = Join-Path $CaptureRoot ('request-{0:D3}.json' -f $requestNumber)
                [IO.File]::WriteAllBytes($requestPath, $bodyBytes)
                $headerFacts = [ordered]@{
                    request_line = $requestLine
                    header_names = $headerNames
                    sensitive_header_names_present = $sensitiveHeadersPresent
                }
                [IO.File]::WriteAllText(
                    (Join-Path $CaptureRoot ('request-{0:D3}-headers.json' -f $requestNumber)),
                    (ConvertTo-Json -InputObject $headerFacts -Depth 5),
                    [Text.UTF8Encoding]::new($false)
                )

                $request = $body | ConvertFrom-Json
                $toolsProperties = @($request.PSObject.Properties | Where-Object { $_.Name -ceq 'tools' })
                $toolsProperty = if ($toolsProperties.Count -eq 1) { $toolsProperties[0] } else { $null }
                $toolsIsJsonArray = $null -ne $toolsProperty -and $toolsProperty.Value -is [Array]
                $tools = if ($toolsIsJsonArray) { @($toolsProperty.Value) } else { @() }
                $nativeImageTools = @($tools | Where-Object {
                    $typeProperties = @($_.PSObject.Properties | Where-Object { $_.Name -ceq 'type' })
                    $typeProperties.Count -eq 1 -and [string]$typeProperties[0].Value -ceq 'image_generation'
                })
                $standaloneImageNamespaces = @($tools | Where-Object {
                    $typeProperty = $_.PSObject.Properties['type']
                    $nameProperty = $_.PSObject.Properties['name']
                    $null -ne $typeProperty -and [string]$typeProperty.Value -eq 'namespace' -and
                    $null -ne $nameProperty -and [string]$nameProperty.Value -eq 'image_gen'
                })
                $includeProperty = $request.PSObject.Properties['include']
                $includeValues = if ($null -eq $includeProperty) { @() } else { @($includeProperty.Value) }
                $imageGenerationIncludes = @($includeValues | Where-Object { [string]$_ -match '^image_generation_call\.' })
                $toolChoiceProperty = $request.PSObject.Properties['tool_choice']
                $toolChoice = if ($null -eq $toolChoiceProperty) { $null } else { $toolChoiceProperty.Value }
                $toolChoiceJson = if ($null -eq $toolChoice) { 'null' } else { ConvertTo-Json -InputObject $toolChoice -Depth 20 -Compress }
                $nativeAdmitted = $toolsIsJsonArray -and $nativeImageTools.Count -gt 0

                $requestFacts = [ordered]@{
                    number = $requestNumber
                    model = [string]$request.model
                    tools_is_json_array = $toolsIsJsonArray
                    tool_count = $tools.Count
                    tool_types = @($tools | ForEach-Object { [string]$_.type })
                    native_image_generation_tool_count = $nativeImageTools.Count
                    native_image_generation_tools = $nativeImageTools
                    standalone_image_gen_namespace_count = $standaloneImageNamespaces.Count
                    image_generation_include_values = $imageGenerationIncludes
                    tool_choice_json = $toolChoiceJson
                    native_image_generation_admitted = $nativeAdmitted
                }
                [IO.File]::WriteAllText(
                    (Join-Path $CaptureRoot ('request-{0:D3}-facts.json' -f $requestNumber)),
                    (ConvertTo-Json -InputObject $requestFacts -Depth 30),
                    [Text.UTF8Encoding]::new($false)
                )

                $responseId = 'resp-beryl-phase13-{0:D3}' -f $requestNumber
                if ($requestNumber -eq 1 -and $nativeAdmitted) {
                    $responseMode = 'hosted_image_generation_call'
                    $outputItem = [ordered]@{
                        type = 'image_generation_call'
                        id = 'ig_beryl_phase13_001'
                        status = 'completed'
                        result = 'Zm9v'
                    }
                }
                else {
                    $responseMode = 'ordinary_assistant_control'
                    $outputItem = [ordered]@{
                        type = 'message'
                        role = 'assistant'
                        id = ('msg-beryl-phase13-{0:D3}' -f $requestNumber)
                        content = @([ordered]@{
                            type = 'output_text'
                            text = ('PHASE13_CONTROL_ASSISTANT_{0:D3}' -f $requestNumber)
                        })
                    }
                }
                [IO.File]::WriteAllText(
                    (Join-Path $CaptureRoot ('response-{0:D3}-mode.txt' -f $requestNumber)),
                    $responseMode,
                    [Text.UTF8Encoding]::new($false)
                )

                $events = @(
                    [ordered]@{ type = 'response.created'; response = [ordered]@{ id = $responseId } },
                    [ordered]@{ type = 'response.output_item.done'; item = $outputItem },
                    [ordered]@{
                        type = 'response.completed'
                        response = [ordered]@{
                            id = $responseId
                            usage = [ordered]@{
                                input_tokens = 0
                                input_tokens_details = $null
                                output_tokens = 0
                                output_tokens_details = $null
                                total_tokens = 0
                            }
                        }
                    }
                )
                $sse = (($events | ForEach-Object {
                    $json = ConvertTo-Json -InputObject $_ -Depth 20 -Compress
                    "event: $($_.type)`ndata: $json`n`n"
                }) -join '')
                $responseBytes = [Text.Encoding]::UTF8.GetBytes($sse)
                $responseHeaders = "HTTP/1.1 200 OK`r`nContent-Type: text/event-stream`r`nContent-Length: $($responseBytes.Length)`r`nConnection: close`r`n`r`n"
                $responseHeaderBytes = [Text.Encoding]::ASCII.GetBytes($responseHeaders)
                $stream.Write($responseHeaderBytes, 0, $responseHeaderBytes.Length)
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

[features]
image_generation = true

[model_providers.beryl_probe]
name = "Beryl Phase 13 Capture Provider"
base_url = "http://127.0.0.1:$port/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
stream_idle_timeout_ms = 30000
"@
$configPath = Join-Path $codexHome 'config.toml'
[IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))

function Add-Check {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [bool] $Passed,
        [Parameter(Mandatory)] [string] $Detail
    )

    $script:Checks.Add([pscustomobject]@{ name = $Name; passed = $Passed; detail = $Detail })
    if (-not $Passed) { throw "Proof check failed: $Name - $Detail" }
}

function New-CasRequestId {
    $id = $script:NextRequestId
    $script:NextRequestId++
    return $id
}

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
    foreach ($credentialName in @('OPENAI_API_KEY', 'CODEX_API_KEY', 'OPENAI_ACCESS_TOKEN')) {
        [void]$psi.EnvironmentVariables.Remove($credentialName)
    }

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
        [IO.File]::AppendAllText(
            (Join-Path $probeRoot 'app-server.stderr.log'),
            $stderr,
            [Text.UTF8Encoding]::new($false)
        )
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
    $script:ProtocolRecords.Add([pscustomobject]@{ Raw = $line; Message = $message })
    return $message
}

function Wait-CasResponse {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [int] $Id
    )

    while ($true) {
        $message = Read-CasMessage -Server $Server
        $idProperty = $message.PSObject.Properties['id']
        if ($null -ne $idProperty -and [int]$idProperty.Value -eq $Id) {
            $errorProperty = $message.PSObject.Properties['error']
            if ($null -ne $errorProperty -and $null -ne $errorProperty.Value) {
                throw ('CAS request failed: ' + ($errorProperty.Value | ConvertTo-Json -Depth 20 -Compress))
            }
            return $message
        }
    }
}

function Invoke-CasRequest {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $Method,
        [Parameter(Mandatory)] [object] $Params
    )

    $id = New-CasRequestId
    Send-CasMessage -Server $Server -Message ([ordered]@{ method = $Method; id = $id; params = $Params })
    return (Wait-CasResponse -Server $Server -Id $id)
}

function Initialize-CasServer {
    param([Parameter(Mandatory)] [object] $Server)

    Send-CasMessage -Server $Server -Message ([ordered]@{
        method = 'initialize'
        id = 0
        params = [ordered]@{
            clientInfo = [ordered]@{
                name = 'beryl_phase13_image_generation_live_probe'
                title = 'Beryl Phase 13 Image Generation Live Probe'
                version = '0.1.0'
            }
            capabilities = [ordered]@{ experimentalApi = $false }
        }
    })
    $response = Wait-CasResponse -Server $Server -Id 0
    Send-CasMessage -Server $Server -Message ([ordered]@{ method = 'initialized'; params = @{} })
    return [string]$response.result.userAgent
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
        if ($null -ne $methodProperty -and [string]$methodProperty.Value -eq 'turn/completed' -and
            [string]$message.params.threadId -eq $ThreadId -and
            [string]$message.params.turn.id -eq $TurnId) {
            if ([string]$message.params.turn.status -ne 'completed') {
                throw "Turn $TurnId completed with status $($message.params.turn.status)."
            }
            return $message
        }
    }
}

function Start-ProbeThread {
    param([Parameter(Mandatory)] [object] $Server)

    $response = Invoke-CasRequest -Server $Server -Method 'thread/start' -Params ([ordered]@{
        cwd = $workRoot
        model = 'gpt-5.4'
        modelProvider = 'beryl_probe'
        developerInstructions = 'Beryl Phase 13 admission probe. Do not invoke any tool.'
        ephemeral = $false
        approvalPolicy = 'never'
        sandbox = 'read-only'
    })
    return [string]$response.result.thread.id
}

function Start-ProbeTurn {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId,
        [Parameter(Mandatory)] [string] $Text
    )

    $response = Invoke-CasRequest -Server $Server -Method 'turn/start' -Params ([ordered]@{
        threadId = $ThreadId
        input = @([ordered]@{ type = 'text'; text = $Text })
    })
    $turnId = [string]$response.result.turn.id
    [void](Wait-TurnCompletion -Server $Server -ThreadId $ThreadId -TurnId $turnId)
    return $turnId
}

function Read-PublicThread {
    param(
        [Parameter(Mandatory)] [object] $Server,
        [Parameter(Mandatory)] [string] $ThreadId
    )

    return (Invoke-CasRequest -Server $Server -Method 'thread/read' -Params ([ordered]@{
        threadId = $ThreadId
        includeTurns = $true
    }))
}

function Wait-CaptureCount {
    param([Parameter(Mandatory)] [int] $Count)

    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (@(Get-ChildItem -LiteralPath $captureRoot -Filter 'request-*.json' -File).Count -lt $Count) {
        if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for provider request count $Count." }
        Start-Sleep -Milliseconds 50
    }
}

function Read-Capture {
    param([Parameter(Mandatory)] [int] $Number)

    Wait-CaptureCount -Count $Number
    $requestPath = Join-Path $captureRoot ('request-{0:D3}.json' -f $Number)
    $factsPath = Join-Path $captureRoot ('request-{0:D3}-facts.json' -f $Number)
    $headersPath = Join-Path $captureRoot ('request-{0:D3}-headers.json' -f $Number)
    return [pscustomobject]@{
        RequestPath = $requestPath
        Raw = [IO.File]::ReadAllText($requestPath, [Text.Encoding]::UTF8)
        Json = (Get-Content -Raw -LiteralPath $requestPath | ConvertFrom-Json)
        Facts = (Get-Content -Raw -LiteralPath $factsPath | ConvertFrom-Json)
        Headers = (Get-Content -Raw -LiteralPath $headersPath | ConvertFrom-Json)
        ResponseMode = [IO.File]::ReadAllText(
            (Join-Path $captureRoot ('response-{0:D3}-mode.txt' -f $Number)),
            [Text.Encoding]::UTF8
        )
    }
}

function Save-ProtocolArtifacts {
    $stdoutLines = @($script:ProtocolRecords | ForEach-Object { $_.Raw })
    $notificationLines = @($script:ProtocolRecords | Where-Object {
        $null -ne $_.Message.PSObject.Properties['method'] -and
        $null -eq $_.Message.PSObject.Properties['id']
    } | ForEach-Object { $_.Raw })
    $stdoutText = if ($stdoutLines.Count -eq 0) { '' } else { ($stdoutLines -join "`n") + "`n" }
    $notificationText = if ($notificationLines.Count -eq 0) { '' } else { ($notificationLines -join "`n") + "`n" }
    [IO.File]::WriteAllText(
        (Join-Path $probeRoot 'app-server-v2-stdout.jsonl'),
        $stdoutText,
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $probeRoot 'app-server-v2-notifications.jsonl'),
        $notificationText,
        [Text.UTF8Encoding]::new($false)
    )
}

function Get-ItemNotifications {
    param(
        [Parameter(Mandatory)] [string] $TurnId,
        [Parameter(Mandatory)] [string] $ItemType
    )

    return @($script:ProtocolRecords | ForEach-Object { $_.Message } | Where-Object {
        $methodProperty = $_.PSObject.Properties['method']
        $null -ne $methodProperty -and [string]$methodProperty.Value -in @('item/started', 'item/completed') -and
        [string]$_.params.turnId -eq $TurnId -and [string]$_.params.item.type -eq $ItemType
    })
}

function Get-PublicThreadItems {
    param([Parameter(Mandatory)] [object] $ThreadReadResponse)

    $items = [Collections.Generic.List[object]]::new()
    foreach ($turn in @($ThreadReadResponse.result.thread.turns)) {
        foreach ($item in @($turn.items)) { $items.Add($item) }
    }
    return $items.ToArray()
}

function Get-RequestInputItemsByType {
    param(
        [Parameter(Mandatory)] [object] $Capture,
        [Parameter(Mandatory)] [string] $Type
    )

    return @($Capture.Json.input | Where-Object {
        $typeProperty = $_.PSObject.Properties['type']
        $null -ne $typeProperty -and [string]$typeProperty.Value -eq $Type
    })
}

$server = Start-CasServer
    $userAgent = Initialize-CasServer -Server $server
    Add-Check -Name 'installed-runtime-version' -Passed ($userAgent.Contains('0.144.1')) -Detail "user_agent=$userAgent"

    $capabilitiesResponse = Invoke-CasRequest -Server $server -Method 'modelProvider/capabilities/read' -Params @{}
    $capabilities = $capabilitiesResponse.result
    Add-Check -Name 'provider-declares-image-generation-capability' -Passed (
        [bool]$capabilities.imageGeneration
    ) -Detail (ConvertTo-Json -InputObject $capabilities -Compress)

    $modelsResponse = Invoke-CasRequest -Server $server -Method 'model/list' -Params ([ordered]@{
        includeHidden = $true
        limit = 100
    })
    $models = @($modelsResponse.result.data)
    $selectedModels = @($models | Where-Object { [string]$_.model -eq 'gpt-5.4' })
    Add-Check -Name 'selected-model-catalog-entry-exactly-one' -Passed ($selectedModels.Count -eq 1) -Detail "matches=$($selectedModels.Count)"
    $selectedModel = $selectedModels[0]
    $selectedModalities = @($selectedModel.inputModalities | ForEach-Object { [string]$_ })
    Add-Check -Name 'selected-model-accepts-image-input' -Passed ('image' -in $selectedModalities) -Detail "modalities=$($selectedModalities -join ',')"

    $threadId = Start-ProbeThread -Server $server
    $turn1 = Start-ProbeTurn -Server $server -ThreadId $threadId -Text 'PHASE13_HOSTED_IMAGE_ADMISSION_PROBE_001'
    $capture1 = Read-Capture -Number 1
    $threadAfterTurn1 = Read-PublicThread -Server $server -ThreadId $threadId

    $turn2 = Start-ProbeTurn -Server $server -ThreadId $threadId -Text 'PHASE13_HISTORY_PROBE_002'
    $capture2 = Read-Capture -Number 2
    $threadAfterTurn2 = Read-PublicThread -Server $server -ThreadId $threadId
    Save-ProtocolArtifacts

    foreach ($captureEntry in @(
        [pscustomobject]@{ Capture = $capture1; Number = 1 },
        [pscustomobject]@{ Capture = $capture2; Number = 2 }
    )) {
        $capture = $captureEntry.Capture
        $number = $captureEntry.Number
        Add-Check -Name "provider-request-$number-loopback-responses-path" -Passed (
            [string]$capture.Headers.request_line -eq 'POST /v1/responses HTTP/1.1'
        ) -Detail "request_line=$($capture.Headers.request_line)"
        Add-Check -Name "provider-request-$number-has-no-sensitive-auth-header" -Passed (
            @($capture.Headers.sensitive_header_names_present).Count -eq 0
        ) -Detail "sensitive_header_count=$(@($capture.Headers.sensitive_header_names_present).Count)"
        Add-Check -Name "provider-request-$number-selected-model" -Passed (
            [string]$capture.Facts.model -eq 'gpt-5.4'
        ) -Detail "model=$($capture.Facts.model)"
    }

    $nativeAdmitted = [bool]$capture1.Facts.native_image_generation_admitted
    $nativeCount1 = [int]$capture1.Facts.native_image_generation_tool_count
    $nativeCount2 = [int]$capture2.Facts.native_image_generation_tool_count
    Add-Check -Name 'responses-tools-is-json-array' -Passed (
        [bool]$capture1.Facts.tools_is_json_array -and [bool]$capture2.Facts.tools_is_json_array
    ) -Detail "first=$($capture1.Facts.tools_is_json_array) second=$($capture2.Facts.tools_is_json_array)"
    Add-Check -Name 'native-admission-derived-only-from-native-tool' -Passed (
        $nativeAdmitted -eq ($nativeCount1 -gt 0)
    ) -Detail "admitted=$nativeAdmitted native_tool_count=$nativeCount1"
    Add-Check -Name 'native-admission-stable-across-continuation' -Passed (
        ($nativeCount1 -gt 0) -eq ($nativeCount2 -gt 0)
    ) -Detail "first=$nativeCount1 second=$nativeCount2"
    Add-Check -Name 'mock-response-obeys-admission-guard' -Passed (
        ($nativeAdmitted -and $capture1.ResponseMode -eq 'hosted_image_generation_call') -or
        (-not $nativeAdmitted -and $capture1.ResponseMode -eq 'ordinary_assistant_control')
    ) -Detail "admitted=$nativeAdmitted response_mode=$($capture1.ResponseMode)"

    $imageNotifications1 = @(Get-ItemNotifications -TurnId $turn1 -ItemType 'imageGeneration')
    $assistantNotifications1 = @(Get-ItemNotifications -TurnId $turn1 -ItemType 'agentMessage')
    $publicItems1 = @(Get-PublicThreadItems -ThreadReadResponse $threadAfterTurn1)
    $publicImages1 = @($publicItems1 | Where-Object { [string]$_.type -eq 'imageGeneration' })
    $publicItems2 = @(Get-PublicThreadItems -ThreadReadResponse $threadAfterTurn2)
    $publicImages2 = @($publicItems2 | Where-Object { [string]$_.type -eq 'imageGeneration' })
    $historyImages = @(Get-RequestInputItemsByType -Capture $capture2 -Type 'image_generation_call')

    if ($nativeAdmitted) {
        $imageMethods = @($imageNotifications1 | ForEach-Object { [string]$_.method })
        Add-Check -Name 'hosted-image-has-no-ordinary-item-lifecycle' -Passed (
            $imageMethods.Count -eq 0
        ) -Detail "methods=$($imageMethods -join ',')"
        Add-Check -Name 'hosted-image-absent-from-public-item-history' -Passed (
            $publicImages1.Count -eq 0 -and $publicImages2.Count -eq 0
        ) -Detail "after_first=$($publicImages1.Count) after_second=$($publicImages2.Count)"
        Add-Check -Name 'hosted-image-provider-history-replayed-once' -Passed (
            $historyImages.Count -eq 1 -and
            [string]$historyImages[0].id -eq 'ig_beryl_phase13_001' -and
            [string]$historyImages[0].status -eq 'completed' -and
            [string]$historyImages[0].result -eq 'Zm9v'
        ) -Detail "history_count=$($historyImages.Count)"
        $hostedLifecycle = 'no_item_lifecycle_notifications'
        $hostedHistory = 'provider_history_only_public_item_absent'
        $boundaryConclusion = 'native_hosted_image_generation_admitted_without_public_item_lifecycle'
        $limitations = @()
    }
    else {
        Add-Check -Name 'unadmitted-hosted-image-never-emitted' -Passed (
            $imageNotifications1.Count -eq 0
        ) -Detail "image_notification_count=$($imageNotifications1.Count)"
        Add-Check -Name 'unadmitted-hosted-image-absent-from-public-history' -Passed (
            $publicImages1.Count -eq 0 -and $publicImages2.Count -eq 0
        ) -Detail "after_first=$($publicImages1.Count) after_second=$($publicImages2.Count)"
        Add-Check -Name 'unadmitted-hosted-image-absent-from-provider-history' -Passed (
            $historyImages.Count -eq 0
        ) -Detail "history_count=$($historyImages.Count)"
        $assistantMethods = @($assistantNotifications1 | ForEach-Object { [string]$_.method })
        Add-Check -Name 'control-assistant-ordinary-lifecycle-exact' -Passed (
            ($assistantMethods -join ',') -eq 'item/started,item/completed'
        ) -Detail "methods=$($assistantMethods -join ',')"
        Add-Check -Name 'no-standalone-imagegen-substitution-at-boundary' -Passed (
            [int]$capture1.Facts.standalone_image_gen_namespace_count -eq 0
        ) -Detail "namespace_count=$($capture1.Facts.standalone_image_gen_namespace_count)"
        Add-Check -Name 'no-image-result-include-without-native-tool' -Passed (
            @($capture1.Facts.image_generation_include_values).Count -eq 0
        ) -Detail "include_count=$(@($capture1.Facts.image_generation_include_values).Count)"
        $hostedLifecycle = 'not_exercised_because_native_tool_not_admitted'
        $hostedHistory = 'not_created_because_mock_guard_refused_unadmitted_call'
        $boundaryConclusion = 'native_hosted_image_generation_not_admitted'
        $limitations = @(
            'The hosted ImageGenerationCall parser/lifecycle path was intentionally not stimulated because the outbound request did not admit the native tool.',
            'This proof does not cover the separate standalone image_gen.imagegen extension or its Images API backend.'
        )
    }

    Add-Check -Name 'continuation-response-is-control-only' -Passed (
        $capture2.ResponseMode -eq 'ordinary_assistant_control'
    ) -Detail "response_mode=$($capture2.ResponseMode)"

    $notificationPath = Join-Path $probeRoot 'app-server-v2-notifications.jsonl'
    $stdoutPath = Join-Path $probeRoot 'app-server-v2-stdout.jsonl'
    $nativeToolsJson = ConvertTo-Json -InputObject @($capture1.Facts.native_image_generation_tools) -Depth 30 -Compress
    $toolTypesJson = ConvertTo-Json -InputObject @($capture1.Facts.tool_types) -Compress
    $captureEvidence = @(Get-ChildItem -LiteralPath $captureRoot -File | Where-Object {
        $_.Name -ne 'stop'
    } | Sort-Object Name | ForEach-Object {
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
        source_reference_commit = '44918ea10c0f99151c6710411b4322c2f5c96bea'
        probe_script_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $PSCommandPath).Hash
        probe_root = $probeRoot
        report_path = $reportPath
        config_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $configPath).Hash
        selected_provider = 'beryl_probe'
        selected_model = 'gpt-5.4'
        selected_model_input_modalities = $selectedModalities
        provider_capabilities = $capabilities
        credential_boundary = [ordered]@{
            isolated_codex_home = $codexHome
            requires_openai_auth = $false
            sensitive_provider_request_headers_present = @($capture1.Headers.sensitive_header_names_present)
            endpoint = "http://127.0.0.1:$port/v1/responses"
        }
        outbound_request = [ordered]@{
            bytes = [Text.Encoding]::UTF8.GetByteCount($capture1.Raw)
            sha256 = Get-Utf8Sha256 -Text $capture1.Raw
            tools_is_json_array = [bool]$capture1.Facts.tools_is_json_array
            tool_count = [int]$capture1.Facts.tool_count
            tool_types_json = $toolTypesJson
            native_image_generation_tool_count = $nativeCount1
            native_image_generation_tools_json = $nativeToolsJson
            native_image_generation_tools_sha256 = Get-Utf8Sha256 -Text $nativeToolsJson
            standalone_image_gen_namespace_count = [int]$capture1.Facts.standalone_image_gen_namespace_count
            image_generation_include_values = @($capture1.Facts.image_generation_include_values)
            tool_choice_json = [string]$capture1.Facts.tool_choice_json
        }
        semantic_result = [ordered]@{
            boundary_conclusion = $boundaryConclusion
            native_hosted_image_generation_admitted = $nativeAdmitted
            mock_first_response_mode = $capture1.ResponseMode
            hosted_image_generation_call_sent = ($capture1.ResponseMode -eq 'hosted_image_generation_call')
            hosted_item_lifecycle = $hostedLifecycle
            hosted_history = $hostedHistory
            control_assistant_lifecycle_observed = (-not $nativeAdmitted)
        }
        app_server_v2_notifications = [ordered]@{
            bytes = (Get-Item -LiteralPath $notificationPath).Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $notificationPath).Hash
            raw_path = $notificationPath
            full_stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash
        }
        captures = $captureEvidence
        checks = $script:Checks.ToArray()
        limitations = $limitations
    }
    $reportJson = ConvertTo-Json -InputObject $report -Depth 30
    [IO.File]::WriteAllText($reportPath, $reportJson, [Text.UTF8Encoding]::new($false))
    Write-Output $reportJson
}
finally {
    try { Save-ProtocolArtifacts } catch {}
    [IO.File]::WriteAllText(
        (Join-Path $probeRoot 'checks.partial.json'),
        (ConvertTo-Json -InputObject $script:Checks.ToArray() -Depth 10),
        [Text.UTF8Encoding]::new($false)
    )
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
    if (-not $KeepArtifacts -and $script:Checks.Count -gt 0 -and
        -not ($script:Checks | Where-Object { -not $_.passed })) {
        Remove-Item -LiteralPath $probeRoot -Recurse -Force
    }
}
