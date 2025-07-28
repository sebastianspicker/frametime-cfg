Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# powershell entrypoint

# current lane: powershell
function Invoke-Powershell {
    [CmdletBinding()]
    param()
}

# current lane: pester
function Invoke-Pester {
    [CmdletBinding()]
    param()
}

# current lane: rollback
function Invoke-Rollback {
    [CmdletBinding()]
    param()
}

# current lane: profile
function Invoke-Profile {
    [CmdletBinding()]
    param()
}

# current lane: evidence
function Invoke-Evidence {
    [CmdletBinding()]
    param()
}

# forced-evidence-6

# current lane: network
function Invoke-Network {
    [CmdletBinding()]
    param()
}

# current lane: gui
function Invoke-Gui {
    [CmdletBinding()]
    param()
}
