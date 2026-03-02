# Branch Protection Rules

This document defines the branch protection configuration for the agentd repository.

## Overview

Branch protection rules ensure code quality and prevent unintended changes to protected branches. The rules are enforced via GitHub's branch protection settings and CI pipeline gates.

## Protected Branches

| Branch | Protection Level | Reason |
|--------|-----------------|--------|
| `main` | Strict | Primary release branch |
| `master` | Strict | Alternative main branch |

## Required Checks

The following GitHub Actions jobs must pass before merging to protected branches:

### Gate-to-Check Mapping

| CI Job | Required Check | Description |
|--------|---------------|-------------|
| `preflight` | ✅ Required | Baseline file validation |
| `build-gate` | ✅ Required | Cargo workspace build (T4) |
| `test-gate` | ✅ Required | Unit/integration tests (T4) |
| `security-gate` | ✅ Required | Security scanning (T4) |
| `phase-a-gate` | ✅ Required | Phase A quantitative gate (T12) |
| `gate-syscall` | ✅ Required | System call validation (T4) |
| `gate-isolation` | ✅ Required | cgroup/systemd isolation runtime check (T4) |

### Strict Required Checks

1. **All checks required**: Requires all status checks to pass before merging
2. **Up-to-date branches**: Requires branches to be up-to-date before merging
3. **Dismiss stale reviews**: Automatically dismisses reviews when new commits are pushed

## Branch Protection Settings (GitHub UI)

Navigate to: Settings → Branches → Branch protection rules → Add rule

### Rule Configuration

```
Pattern: main
✓ Require pull request reviews before merging
    Required approving reviews: 1
    ✓ Dismiss stale reviews when new commits are pushed
    ✓ Require review from code owners

    ✓ Require status checks to pass before merging
    ✓ Require branches to be up to date before merging
    Select status checks:
      - CI Gates / Preflight Check (required)
      - CI Gates / Build Gate (required)
      - CI Gates / Test Gate (required)
      - CI Gates / Security Gate (required)
      - CI Gates / Phase A Gate (required)
      - CI Gates / System Call Gate (required)
      - CI Gates / Isolation Gate (Ubuntu 25.10) (required)

✓ Require conversation resolution before merging

✓ Include administrators (enforce for everyone)
```

### For master branch

Duplicate the same rule pattern for `master`.

## CI Gate Evidence

Each CI gate job uploads evidence to `.sisyphus/evidence/`:

```
.sisyphus/evidence/
├── evidence-preflight/
├── evidence-build-gate/
├── evidence-test-gate/
├── evidence-security-gate/
├── evidence-phase-a-gate/
├── evidence-gate-syscall/
├── evidence-gate-isolation/
└── evidence-all/
```

Evidence artifacts are retained for 30 days to support debugging and audit trails.

## Local Validation

Before pushing, validate the gate configuration locally:

```bash
bash scripts/gate-check.sh
```

This checks:
- Workflow file exists and has correct structure
- All required jobs are defined
- Evidence upload steps are present
- Shell scripts have valid syntax

## Additions to Protected Branches

**Direct pushes to protected branches are BLOCKED.**

To make changes:
1. Create a feature branch
2. Open a pull request
3. Pass all required CI checks
4. Get required review approval
5. Squash and merge

## Emergency Bypass

In case of critical hotfixes that bypass normal CI:

1. **Temporary disable protection** (admin only)
2. **Push the fix directly**
3. **Re-enable protection immediately**
4. **Document the bypass** in team channel

> ⚠️ **Warning**: This should be extremely rare and always documented.

## Future Enhancements

- Add code owner review requirements for specific paths
- Require signed commits
- Add dependency review checks
- Add secret scanning validation
