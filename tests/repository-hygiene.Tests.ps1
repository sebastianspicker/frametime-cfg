BeforeAll {
    Set-StrictMode -Version Latest
    $script:RepoRoot = Split-Path $PSScriptRoot -Parent
    $script:GitDir = Join-Path $script:RepoRoot ".git"

    function script:Get-PublicMarkdownFiles {
        if (Test-Path -LiteralPath $script:GitDir) {
            return @(& git -C $script:RepoRoot ls-files --cached --others --exclude-standard "*.md" |
                ForEach-Object { Get-Item -LiteralPath (Join-Path $script:RepoRoot $_) })
        }
        return @(Get-ChildItem -Path $script:RepoRoot -Recurse -File -Filter *.md |
            Where-Object {
                $_.FullName -notmatch '[/\\]docs[/\\](agent|archive)[/\\]'
            })
    }
}

Describe "Public repository hygiene" {

    It "does not track files that the repository ignore policy marks private or generated" {
        if (-not (Test-Path -LiteralPath $script:GitDir)) {
            Set-ItResult -Skipped -Because "Tracked-file hygiene requires a Git checkout."
            return
        }

        $trackedIgnored = @(& git -C $script:RepoRoot ls-files -ci --exclude-standard)
        $LASTEXITCODE | Should -Be 0
        $trackedIgnored | Should -BeNullOrEmpty
    }

    It "does not track local audit, agent, cache, runtime, or credential lanes" {
        if (-not (Test-Path -LiteralPath $script:GitDir)) {
            Set-ItResult -Skipped -Because "Tracked-file hygiene requires a Git checkout."
            return
        }

        $tracked = @(& git -C $script:RepoRoot ls-files)
        $LASTEXITCODE | Should -Be 0
        $privatePathPattern = @(
            '^(docs/(agent|archive|source-audit)|private|local|scratch|tmp|temp)/'
            '^(\.agents|\.codex|\.codegraph|\.serena)/'
            '(^|/)(state|progress|backup|benchmark_history|latency_history)\.json$'
            '(^|/)(testResults|test-results|junit)(-[^/]*)?\.xml$'
            '\.(pem|key|p12|pfx|sarif|jsonl|clixml|etl|evtx|dmp|har)$'
        ) -join '|'

        @($tracked | Where-Object { $_ -match $privatePathPattern }) |
            Should -BeNullOrEmpty
    }

    It "does not publish obsolete GUI screenshots" {
        Test-Path -LiteralPath (Join-Path $script:RepoRoot "docs/screenshots") |
            Should -BeFalse

        $publicMarkdown = Get-PublicMarkdownFiles
        foreach ($file in $publicMarkdown) {
            (Get-Content -LiteralPath $file.FullName -Raw) |
                Should -Not -Match 'docs/screenshots|screenshots/[0-9]'
        }
    }

    It "keeps relative links in public Markdown resolvable" {
        $missing = [System.Collections.Generic.List[string]]::new()
        $publicMarkdown = Get-PublicMarkdownFiles

        foreach ($file in $publicMarkdown) {
            $source = Get-Content -LiteralPath $file.FullName -Raw
            foreach ($match in [regex]::Matches($source, '!?\[[^\]]*\]\(([^)]+)\)')) {
                $target = $match.Groups[1].Value.Trim()
                if ($target -match '^(https?://|mailto:|#)') { continue }
                $relativePath = ($target -split '#', 2)[0]
                if (-not $relativePath) { continue }
                if (-not (Test-Path -LiteralPath (Join-Path $file.DirectoryName $relativePath))) {
                    $missing.Add("$($file.FullName): $relativePath")
                }
            }
        }

        $missing | Should -BeNullOrEmpty
    }
}
