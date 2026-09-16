# ADR 001 — Tauri 2 for the Desktop Shell

- Status: accepted (2026-09-17)
- Spec: `product.md` §4, §31; Master Spec §1

## Context

ForgeConvert needs a cross-platform desktop shell (Windows first) around a
Rust conversion core, with drag-drop, file pickers, progress UI, and
installers. Verified on this machine: VS Build Tools 2022 (MSVC 14.44) +
WinSDK 10.0.26100.0 + WebView2 153 — all Tauri 2 Windows prerequisites
present ([prerequisites](https://v2.tauri.app/start/prerequisites/)).

## Decision

Use **Tauri 2** (`tauri 2.11`, `@tauri-apps/api 2.11`, CLI 2.11.4).
Frontend calls Rust only via job-level commands
(`create_conversion_job`, `start_job`, `cancel_job`, `get_job_status`,
`get_format_capabilities`, `get_file_info`); backend stays authoritative.

## Alternatives

- Electron: rejected — Chromium bundling is heavy for a file utility;
  team/toolchain already Rust-oriented.
- Native WinUI/Qt: rejected — second language + weaker web-UI velocity.
- Tauri 3 alpha: rejected — pre-release, do not ship on alpha.

## Consequences

- Small binaries, OS webview, MSI+NSIS via `tauri build` (Phase 6/10).
- Tauri officially supports MSVC targets only — our MSVC toolchain satisfies this.
- Capability files must gate dialog/fs permissions (least privilege).
