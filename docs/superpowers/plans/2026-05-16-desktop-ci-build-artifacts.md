# Desktop CI Build Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build unsigned macOS and Windows Tauri desktop app artifacts on pushes to `main` and upload them as GitHub Actions workflow artifacts.

**Architecture:** Add one GitHub Actions workflow with a two-platform matrix. The workflow installs Node 25, Rust stable, npm dependencies for both desktop package roots, then delegates platform bundling and artifact upload to `tauri-apps/tauri-action`.

**Tech Stack:** GitHub Actions, Tauri 2, Node 25, npm, Rust stable.

---

## File Structure

- Create: `.github/workflows/desktop-build.yml`
  - Defines push-to-main and manual desktop build workflow.
- Modify: `clients/desktop/README.md`
  - Pins the documented Node prerequisite to Node 25.

### Task 1: Add Desktop Build Workflow

**Files:**
- Create: `.github/workflows/desktop-build.yml`

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/desktop-build.yml` with a two-platform matrix, Node 25 setup, dependency installation, and Tauri artifact upload.

- [ ] **Step 2: Validate YAML**

Run:

```bash
python3 - <<'PY'
import yaml
with open(".github/workflows/desktop-build.yml", "r", encoding="utf-8") as f:
    yaml.safe_load(f)
print("ok")
PY
```

Expected: `ok`.

### Task 2: Pin Desktop Node Version Documentation

**Files:**
- Modify: `clients/desktop/README.md`

- [ ] **Step 1: Update prerequisites**

Change the desktop prerequisite from `Node 20+` to `Node 25`.

- [ ] **Step 2: Verify README references**

Run: `rg -n "Node 20|Node 25|node-version" clients/desktop/README.md .github/workflows/desktop-build.yml`

Expected: no `Node 20` references, and both README and workflow reference Node 25.

### Task 3: Local Verification

**Files:**
- Read: `clients/desktop/ui/package.json`
- Read: `clients/desktop/package.json`

- [ ] **Step 1: Check local Node version**

Run: `node --version`.

Expected: version starts with `v25.` for full local parity. If not, report that CI is pinned to Node 25 but local verification used the installed Node version.

- [ ] **Step 2: Run UI production build**

Run: `npm --prefix clients/desktop/ui run build`.

Expected: TypeScript and Vite build complete successfully.
