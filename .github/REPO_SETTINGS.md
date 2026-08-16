# GitHub repository settings checklist

These settings are not enforced by repository files and were not verified
against the remote repository during alpha preparation. A repository
administrator should review each item before publication.

## Main branch protection

- [ ] Require a pull request before merging.
- [ ] Require at least one approval.
- [ ] Dismiss stale approvals after new commits.
- [ ] Require review from Code Owners where appropriate.
- [ ] Require the applicable checks from `lint.yml`, `security.yml`, and
  `rust.yml`.
- [ ] Require conversation resolution.
- [ ] Restrict bypass permissions to the intended maintainers.
- [ ] Decide whether signed commits are required.

Required checks: `Browser demonstration checks`, `PSScriptAnalyzer`, `Verify syntax (parse check)`, `Windows PowerShell 5.1 compatibility`, `Pester tests`, `Pester tests (macOS)`, `EstimateKey cross-reference`, `E2E process smoke`, `Entry point smoke tests`, `Secret & credential detection`, `PowerShell safety patterns`, `Workflow file integrity`, `Frametime Rust host gates`, `Frametime Rust Windows gates`, `Northclock source gates`, `Driver Foundry source gates`, `Driver Foundry Windows tests`.

Do not copy check names from this document without comparing them with the
current workflow job names in `.github/workflows/`.

## Actions permissions

- [ ] Require approval for workflows from first-time external contributors.
- [ ] Set the default workflow token to read-only repository contents.
- [ ] Keep permission for Actions to create or approve pull requests disabled
  unless a reviewed workflow requires it.

## Security features

- [ ] Enable private vulnerability reporting.
- [ ] Enable secret scanning if available for the repository visibility and
  account plan.
- [ ] Enable push protection if available.
- [ ] Enable Dependabot alerts.
- [ ] Review weekly GitHub Actions updates configured in
  `.github/dependabot.yml` before merging them.

## Access

- [ ] Limit write and administration access to current maintainers.
- [ ] Review deploy keys, GitHub Apps, webhooks, environments, and repository
  secrets before making the repository public.
- [ ] Confirm that external pull requests receive only the permissions required
  by the checked-in workflows.
