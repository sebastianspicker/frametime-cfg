# Required GitHub Repository Settings

These settings cannot be enforced via code — they must be configured manually in the GitHub web UI under **Settings**.

## Branch Protection Rules (Settings > Branches)

Create a rule for the `main` branch:

- [x] **Require a pull request before merging**
  - [x] Require approvals: 1
  - [x] Dismiss stale pull request approvals when new commits are pushed
  - [x] Require review from Code Owners
- [x] **Require status checks to pass before merging**
  - Required checks: `PSScriptAnalyzer`, `Secret & credential detection`, `PowerShell safety patterns`, `Workflow file integrity`
- [x] **Require conversation resolution before merging**
- [x] **Do not allow bypassing the above settings** (even for admins)
- [ ] Require signed commits *(optional — adds trust but complicates workflow)*

## Actions Permissions (Settings > Actions > General)

- **Fork pull request workflows**: Require approval for first-time contributors
- **Workflow permissions**: Read repository contents (default)
- **Allow GitHub Actions to create and approve pull requests**: Disabled

## Secret Scanning (Settings > Code security and analysis)

- [x] **Secret scanning**: Enabled
- [x] **Push protection**: Enabled *(blocks pushes containing detected secrets)*
- [x] **Dependabot alerts**: Enabled *(not critical for this repo but good practice)*

## Dependabot (automatic)

Configured in `.github/dependabot.yml`:
- **GitHub Actions**: Weekly checks for pinned SHA updates (security patches)
- Grouped into single PRs to reduce noise

## Collaborator Permissions

- Limit **Write** access to trusted maintainers only
- External contributors should submit PRs from forks (read-only CI by default)
