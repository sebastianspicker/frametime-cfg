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
                $_.FullName -notmatch '[/\\](?:\.internal|docs[/\\]archive)[/\\]'
            })
    }

    function script:Get-PublicTextFiles {
        $extensions = @('.ps1', '.psd1', '.bat', '.cmd', '.cfg', '.xaml', '.md', '.yml', '.yaml')
        if (Test-Path -LiteralPath $script:GitDir) {
            return @(& git -C $script:RepoRoot ls-files --cached --others --exclude-standard |
                ForEach-Object { Join-Path $script:RepoRoot $_ } |
                Where-Object {
                    (Test-Path -LiteralPath $_ -PathType Leaf) -and
                    (([IO.Path]::GetExtension($_) -in $extensions) -or ([IO.Path]::GetFileName($_) -eq 'LICENSE'))
                } |
                ForEach-Object { Get-Item -LiteralPath $_ })
        }

        return @(Get-ChildItem -Path $script:RepoRoot -Recurse -File |
            Where-Object {
                $_.FullName -notmatch '[/\\](?:\.internal|docs[/\\]archive)[/\\]' -and
                (($_.Extension -in $extensions) -or $_.Name -eq 'LICENSE')
            })
    }
}

Describe "Public repository contract" {

    It "does not track files that the repository ignore policy marks private or generated" {
        if (-not (Test-Path -LiteralPath $script:GitDir)) {
            Set-ItResult -Skipped -Because "Tracked-file hygiene requires a Git checkout."
            return
        }

        $trackedIgnored = @(& git -C $script:RepoRoot ls-files -ci --exclude-standard)
        $LASTEXITCODE | Should -Be 0
        $trackedIgnored | Should -BeNullOrEmpty
    }

    It "does not track private workspace, cache, runtime, or credential paths" {
        if (-not (Test-Path -LiteralPath $script:GitDir)) {
            Set-ItResult -Skipped -Because "Tracked-file hygiene requires a Git checkout."
            return
        }

        $tracked = @(& git -C $script:RepoRoot ls-files)
        $LASTEXITCODE | Should -Be 0
        $privatePathPattern = @(
            '^(\.internal|private|local|scratch|tmp|temp)/'
            '(^|/)(state|progress|backup|benchmark_history|latency_history)\.json$'
            '(^|/)(testResults|test-results|junit)(-[^/]*)?\.xml$'
            '\.(pem|key|p12|pfx|sarif|jsonl|clixml|etl|evtx|dmp|har)$'
        ) -join '|'

        @($tracked | Where-Object { $_ -match $privatePathPattern }) |
            Should -BeNullOrEmpty
    }

    It "keeps bundled Rust projects as tracked ordinary directories" {
        if (-not (Test-Path -LiteralPath $script:GitDir)) {
            Set-ItResult -Skipped -Because "Repository-topology verification requires a Git checkout."
            return
        }

        $submodulePaths = @()
        $gitmodulesPath = Join-Path $script:RepoRoot '.gitmodules'
        if (Test-Path -LiteralPath $gitmodulesPath -PathType Leaf) {
            $submodulePaths = @(
                & git -C $script:RepoRoot config --file .gitmodules --get-regexp '^submodule\..*\.path$' 2>$null |
                    ForEach-Object { ($_ -split '\s+', 2)[1] }
            )
        }

        foreach ($directory in @('rust/driver-foundry', 'rust/northclock')) {
            $fullPath = Join-Path $script:RepoRoot $directory
            (Test-Path -LiteralPath $fullPath -PathType Container) | Should -BeTrue
            (Get-Item -LiteralPath $fullPath -Force).LinkType | Should -BeNullOrEmpty
            (Test-Path -LiteralPath (Join-Path $fullPath '.git')) | Should -BeFalse

            $stagedEntries = @(& git -C $script:RepoRoot ls-files --stage -- $directory)
            $LASTEXITCODE | Should -Be 0
            @($stagedEntries | Where-Object { $_ -match '^160000\s' }) | Should -BeNullOrEmpty

            & git -C $script:RepoRoot ls-files --error-unmatch -- "$directory/Cargo.toml" | Out-Null
            $LASTEXITCODE | Should -Be 0
            $submodulePaths | Should -Not -Contain $directory
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

    It "keeps public text free of em dash characters" {
        $violations = foreach ($file in Get-PublicTextFiles) {
            $source = Get-Content -LiteralPath $file.FullName -Raw
            if ($source.Contains([string][char]0x2014)) {
                $file.FullName
            }
        }

        @($violations) | Should -BeNullOrEmpty
    }

    It "keeps public Markdown free of single-word emphasis" {
        $singleWordEmphasis = '(?<!\*)\*\*[\p{L}\p{N}_.:/+-]+\*\*(?!\*)|(?<![\p{L}\p{N}_])__[\p{L}\p{N}_.:/+-]+__(?![\p{L}\p{N}_])|(?<!\*)\*[\p{L}\p{N}_.:/+-]+\*(?!\*)|(?<![\p{L}\p{N}_])_[\p{L}\p{N}_.:/+-]+_(?![\p{L}\p{N}_])'
        $violations = foreach ($file in Get-PublicMarkdownFiles) {
            $source = Get-Content -LiteralPath $file.FullName -Raw
            $prose = [regex]::Replace($source, '(?s)```.*?```', '')
            $prose = [regex]::Replace($prose, '`[^`\r\n]*`', '')
            if ($prose -match $singleWordEmphasis) {
                "$($file.FullName): $($Matches[0])"
            }
        }

        @($violations) | Should -BeNullOrEmpty
    }

    It "does not publish badges or documentation placeholders" {
        $readme = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'README.md') -Raw
        $readme | Should -Not -Match '!\[[^\]]*\]\(https?://[^)]*(?:badge|shields\.io)'

        $placeholderPattern = '(?im)\bTODO\b|coming soon|insert screenshot|future documentation|configuration pending'
        $violations = foreach ($file in Get-PublicMarkdownFiles) {
            $source = Get-Content -LiteralPath $file.FullName -Raw
            if ($source -match $placeholderPattern) {
                "$($file.FullName): $($Matches[0])"
            }
        }

        @($violations) | Should -BeNullOrEmpty
    }
}
