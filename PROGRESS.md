# ForgeConvert — Development Record (PROGRESS.md)

> Step-by-step build log. Every completed step links to its git commit.
> Spec source: `dev/product.md` (2731 lines). Architecture: clean / layered +
> hexagonal (ports & adapters) influence, per spec §§5–6 + Master Spec §2.
> Goal stated by owner: **working product at the end**.

## Git workflow

- One commit per task/step, messages prefixed `step-N:` (+ `plan:`, `docs:`, `feat:`, `test:`, `fix:`, `chore:` where clearer).
- `git log --oneline` is the source of truth for order; this file is the narrative.

## Step log

| # | Date (UTC) | Step | What was done | Commit |
|---|------------|------|---------------|--------|
| 0 | 2026-09-17 | Spec intake | Read `dev/product.md` fully (all 2731 lines: proposal §§1–48 + Master Spec §§1–36). Wrote `AGENTS.md` repo guidelines via 4 parallel scouts. | _pre-git_ |
| 1 | 2026-09-17 | Repo foundation | `git init`, `main` branch, `.gitignore` (Rust/Node/Tauri), this `PROGRESS.md` record. | _see log_ |
| 2 | — | Toolchain | Install Rust via rustup (MSVC target; Build Tools 2022 + MSVC 14.44 found on machine). Verify `cargo --version`, `rustc --version`. | — |
| 3 | — | Stack research | 4 parallel scouts: image codecs / PDF write+render / Tauri+infra crates / Vue+Vite frontend. Facts only, versions+licenses+URLs. | — |
| 4 | — | Detailed plan | `docs/PLAN.md`: phased plan Phase 0→10 with acceptance criteria. Update `AGENTS.md` (replace spec-only notice). | — |
| 5 | — | Scaffold | Cargo workspace + hexagonal layers, ADRs `docs/adr/001–012`, CI, `tests/fixtures`. | — |
| 6+ | — | Build phases | Phase 1 domain → Phase 2 PNG/JPEG/WebP → Phase 3 jobs → Phase 4 batch → Phase 5 PDF → Phase 6 Tauri UI → Phase 7 CLI → Phase 8 optimize → Phase 9 history → Phase 10 hardening. Each phase: code + tests + commit. | — |

## Environment facts (verified, not guessed)

- OS: Windows 11 x64. Disk: 44G free on D:.
- `node v24.14.0`, `bun 1.3.14`, `pnpm 12.4.1`, `git 2.42.0`, `curl 8.21.0` present.
- `cargo`/`rustc`: NOT installed (this step installs them).
- `winget`: NOT available. `code` CLI: not on PATH.
- MinGW `gcc 6.3.0` (32-bit `mingw32` target, 2017): too old — NOT used.
- **VS Build Tools 2022 present** (`C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools`, MSVC 14.44.35207, `cl.exe`+`link.exe` Hostx64/x64 verified) → use default `x86_64-pc-windows-msvc` Rust target. (Earlier "no VS" reading was wrong; `vswhere` proved Build Tools exist.)
- Git identity: `yanmyoaung2004 <ymyo44277@gmail.com>` (pre-configured).

## Decisions so far

- D1: Clean/hexagonal layers (Presentation → Application → Domain → Infrastructure, domain depends on nothing; ports in domain/application, adapters in infrastructure). Rationale: spec mandates it; keeps Tauri/CLI/SQLite swappable.
- D2: Start one modular Cargo workspace, split crates only when boundaries justify it (spec §33 warns against 7 crates on day one).
- D3: MSVC target, not GNU (Build Tools present; Tauri 2 + `rusqlite/bundled` + PDF renderers build reliably on MSVC).
- D4: No code before facts — image/PDF/Tauri crate choices wait for scout results (Step 3).

## Open questions (for Step 3 scouts)

- Q1: `image` crate WebP encode quality API — sufficient, or need `webp`/`mozjpeg`?
- Q2: PDF→image renderer that builds on Windows-MSVC without vendored binaries (`pdfium-render` binary story vs `hayro` purity)?
- Q3: Tauri 2 MSVC requirement citation + `@tauri-apps/api` v2 version.
- Q4: `rusqlite/bundled` on MSVC build time/impact.
