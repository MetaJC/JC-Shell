<div align="center">

<img src="src/assets/logo.png" alt="JC Shell" width="120" />

# JC Shell

**Cross-platform SSH Terminal + AI Ops Assistant**

Windows · macOS · Linux · Android · iOS

<p>
  <a href="#download">Download</a> ·
  <a href="#features">Features</a> ·
  <a href="#build-from-source">Build</a> ·
  <a href="#tech-stack">Tech Stack</a> ·
  <a href="#links">Links</a>
</p>

</div>

---

## Introduction

JC Shell is a cross-platform SSH client for ops engineers, backend developers and
self-hosters. It combines a remote terminal, a graphical file manager, server
monitoring and an AI ops assistant into a single app — one account, one host list,
synced across desktop and mobile, so you can manage your servers from anywhere.

- **Secure** — everything runs over the SSH encrypted channel; credentials are
  encrypted locally and never uploaded
- **Efficient** — terminal and file manager side by side, with batch operations
- **Cross-platform** — Windows / macOS / Linux desktop + Android / iOS mobile
- **Intelligent** — the AI assistant turns natural language into executable
  commands, with every step visible and interruptible

<p align="center">
  <img src="docs/images/agent-demo.gif" alt="JC Shell · terminal + file system sync demo" width="600" />
</p>

## Features

| Module | Highlights |
|---|---|
| Connection management | Hosts (name / address / port / user / password or key), groups, search, one-click connect |
| Remote terminal | Full SSH terminal: multi-tab, ANSI colors, full-screen apps (vim/top), zoom, history |
| File management | SFTP-based: browse, upload/download (drag & drop + queue), inline edit, create/rename/delete, chmod, tar |
| Split view | Terminal × file manager split (vertical/horizontal), synced scrolling, batch UI broadcast |
| Local management | Local shell + local file browser, drag files between local and remote windows |
| Port forwarding | SSH-tunnel-based local port forwarding rules |
| Monitoring | Live CPU / memory / disk / network curves |
| AI assistant | Natural-language commands, step-by-step execution, interrupt anytime, auto mirror fallback on stalled downloads |
| Account sync | Phone login, host config cloud sync (passwords excluded) |
| Extras | System tray, auto update, EN/中文, dark/light themes, biometric unlock (mobile) |

## Download

| Platform | Where |
|---|---|
| Windows / macOS / Linux | [Gitee Releases](https://gitee.com/Open-JC/jc-shell-community/releases) (recommended in China) / [GitHub Releases](https://github.com/MetaJC/JC-Shell-Community/releases) |
| Android | Same as above (APK) |
| iOS | App Store |

> Website: <https://www.openjc.cc/jc-shell> ｜ Docs: <https://docs.openjc.cc>

## Build from Source

### Requirements

- Node.js 20+
- Rust 1.77+
- Platform toolchain: VS 2022 Build Tools (C++) on Windows, Xcode on macOS,
  webkit2gtk & friends on Linux

### Commands

```bash
# install dependencies
npm install

# desktop dev
npm run tauri:dev

# build installer for the current platform
npm run tauri:build:prod

# Android / iOS (first time only)
npm run tauri:android:init
npm run tauri:ios:init
```

> Environment variables such as the backend URL are injected at build time via
> `.env.production` / `.env.development` (template: `.env.example`). These files
> are not tracked by git.

## Tech Stack

| Layer | Technology |
|---|---|
| Framework | Tauri 2 + Rust + Vue 3 + Vite + TypeScript |
| UI | Element Plus + Tailwind CSS |
| Terminal | xterm.js (WebGL renderer) |
| Editor | CodeMirror 6 |
| SSH / SFTP | russh / russh-sftp (pure Rust) |
| Storage | SQLite (sqlx) + OS Keychain (keyring) |
| Crypto | AES-256-GCM (local data), OS Keychain (credentials) |

## Project Layout

Top-level files and directories of this repository (Community Edition); build artifacts ignored by `.gitignore` are omitted:

```
jc-shell-community/
├── src/                          # Vue 3 + TypeScript frontend
│   ├── api/                      # Tauri IPC wrappers (bridge to Rust commands)
│   ├── assets/                   # static assets (logo.png, jc-shell-logo.png)
│   ├── components/               # shared components (terminal, file manager, settings, port forwarding, host list ...)
│   ├── composables/              # composables (drag, shortcuts, session sync, lock, window registry ... use*.ts)
│   ├── router/                   # frontend router (hash mode)
│   ├── stores/                   # Pinia stores (hosts / sessions / AI / sync / settings ...)
│   ├── styles/                   # stylesheets (mobile.css ...)
│   ├── utils/                    # helpers (platform detection / edition / security ...)
│   ├── views/                    # page-level views (main window / lock screen ...)
│   ├── App.vue                   # root component
│   ├── env.d.ts                  # TypeScript ambient declarations (.vue and Vite env vars)
│   ├── main.ts                   # frontend entry (create and mount Vue app, Pinia, Router)
│   └── style.css                 # global base styles
├── src-tauri/                    # Rust backend (Tauri 2)
│   ├── src/                      # Rust sources (commands / services / repositories / models / common ...)
│   ├── capabilities/             # Tauri 2 permission capabilities (default.json / mobile.json)
│   ├── icons/                    # platform packaging icons
│   ├── ios-patches/              # iOS native patches (jc_safearea.mm, safe-area)
│   ├── windows/                  # Windows installer script (hooks.nsh)
│   ├── vendor/                   # vendored patched crates (required to build, must be committed)
│   ├── build.rs                  # Tauri build script
│   ├── Cargo.toml                # Rust dependencies and package config
│   ├── Cargo.lock                # dependency lockfile (must be committed)
│   ├── tauri.conf.json           # main Tauri config
│   ├── tauri.android.conf.json   # Android platform config
│   ├── tauri.ios.conf.json       # iOS platform config
│   ├── tauri.ios.prod.conf.json  # iOS production config
│   ├── tauri.conf.system.json    # desktop system-level config
│   ├── Info.ios.plist            # iOS Info.plist
│   ├── app-icon.png              # app icon source
│   ├── app-icon-mobile.png       # mobile icon
│   ├── app-icon-mobile-src.png   # mobile icon source
│   └── taskbar-source.png        # taskbar icon source
├── scripts/                      # build / release scripts (Node ESM)
│   ├── android-signing.mjs       # Android signing config
│   ├── android-sign-prod.mjs     # Android production signing
│   ├── bump-version.mjs          # bump version across manifests
│   ├── icon-build.mjs            # generate platform icons from source images
│   ├── icon-store.mjs            # icon asset organisation / storage
│   ├── tauri-xcode-env.mjs       # inject Xcode environment for iOS builds
│   └── with-env.mjs              # load .env.* and inject into build commands
├── .env.example                  # environment variable template (backend URL, OAuth client ...; real .env.* not tracked)
├── .gitignore                    # Git ignore rules (ignore build artifacts, keep Cargo.lock and vendor/)
├── index.html                    # Vite frontend HTML entry
├── package.json                  # frontend dependencies and npm scripts (tauri:dev / tauri:build:prod ...)
├── postcss.config.js             # PostCSS config (Tailwind / autoprefixer)
├── tailwind.config.js            # Tailwind CSS config (theme and content scan scope)
├── tsconfig.json                 # TypeScript compiler config
├── vite.config.ts                # Vite build config (with @tauri-apps/cli plugin, dev server settings)
├── LICENSE                       # Apache-2.0 license text
├── README.md                     # 中文说明文档（Chinese documentation）
└── README.en.md                  # English documentation (this file)
```

> `dist/`, `node_modules/`, `.env.development`, `.env.production`, `src-tauri/target/` etc. are ignored by `.gitignore` and not tracked; they are regenerated locally by `npm install` / build.

## Roadmap

- [ ] Remote desktop (VNC over SSH)
- [ ] Jump host / multi-hop SSH chains
- [ ] Command snippets library
- [ ] Session import (migrate from other SSH clients)

## Contributing

Issues and pull requests are welcome. Before submitting:

1. Make sure `npm run build` passes (including `vue-tsc` type checking);
2. Never commit keys, passwords, certificates or internal addresses;
3. Use conventional commit messages (`feat:` / `fix:` / `docs:` / `refactor:` …).

## License

JC Shell Community Edition is released under the **Apache License 2.0**.

- The full license text is in [`LICENSE`](LICENSE).
- Third-party dependencies (including the vendored patched crates) retain their
  original licenses, mostly MIT / Apache-2.0 / BSD. See [`NOTICE`](NOTICE) for details.
- The name "JC Shell" and the JC Shell logo are trademarks of OpenJC and are not
  covered by the Apache-2.0 grant; their use is governed by OpenJC's trademark policy.

Derivative developers are encouraged (but not required) to indicate
"based on JC Shell" in their distribution and UI.

## Links

- Website: <https://www.openjc.cc/jc-shell>
- Docs: <https://docs.openjc.cc>
- Issues: <https://github.com/MetaJC/JC-Shell-Community/issues>
