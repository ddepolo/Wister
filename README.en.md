# Wister

[![CI](https://github.com/ddepolo/Wister/actions/workflows/ci.yml/badge.svg)](https://github.com/ddepolo/Wister/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

*[Documentación completa en español](README.md)*

**Local, free and open-source voice dictation for Windows.** Hold a hotkey, speak, release, and the text is pasted wherever your cursor is: browser, editor, chat or terminal.

Transcription runs **on your PC** with [Whisper](https://github.com/openai/whisper) (through [whisper.cpp](https://github.com/ggml-org/whisper.cpp)). Your audio never leaves your computer: no accounts, no subscription, no telemetry. The network is only used to download the speech model, and only when you ask for it.

> **Status: 0.2.0.** It works and is used daily, but it is an early release and may still change significantly.

## Features

- Global push-to-talk hotkey (left `Ctrl` + `Shift` by default; configurable).
- Pastes into any app and then restores your previous clipboard (images included), without leaving the dictated text in the `Win+V` history.
- Fast: with a GPU, text appears about 250 ms after releasing the hotkey.
- Runs on the GPU through Vulkan (NVIDIA, AMD, Intel) or on the CPU.
- A small waveform overlay that never steals focus from the app you are typing in.
- First-run wizard: language, model download, microphone test and a first dictation.
- Local dictation history (SQLite) with search, copy and delete; it can be turned off and wiped.
- Usage stats: words dictated today, this week and overall, time saved compared to typing, streak and speaking speed.
- Settings: model, microphone (also from the tray menu) and its volume, language, hotkey, sounds and start with Windows.
- Personal dictionary: words Whisper should know (names, brands, jargon) and replacements that always apply, such as "punto y aparte" → line break or removing filler words.
- Diagnostics: a benchmark that tells which model runs best on your PC, and a plain-text report to ask for help (it never includes what you dictated).
- Does not hallucinate text: a voice activity detector (Silero VAD) keeps only speech and trims silences; recordings with only noise are discarded.

## Language

Wister transcribes any language Whisper supports (the UI offers Spanish, English, Portuguese, French, Italian, German and automatic detection). **The user interface and the documentation are in Spanish for now**; code identifiers and comments are mostly in Spanish too.

## Download

Get the installer from the [latest release](https://github.com/ddepolo/Wister/releases/latest): `Wister_<version>_x64-setup.exe` for most PCs (GPU through Vulkan, falling back to the CPU), or `Wister_<version>_x64-cpu-setup.exe` if that one does not start (virtual machines, no video drivers). The installer is not signed yet, so SmartScreen will warn about an unknown publisher: **More info → Run anyway**. Updates are installed from **Configuración → Buscar actualizaciones** (the app never checks on its own).

## Building

To build from source on Windows you need Rust (MSVC), Visual Studio Build Tools (C++), CMake, LLVM, Node.js 24+, and the Vulkan SDK for GPU support:

```powershell
npm install
.\scripts\dev.ps1     # run in development mode (Vulkan; -Cpu for CPU only)
.\scripts\build.ps1   # build the NSIS installer (Vulkan; -Cpu for CPU only)
```

The installer is not signed yet, so SmartScreen will warn about an unknown publisher (*More info → Run anyway*).

## Contributing

Contributions are welcome, in Spanish or English. See [`CONTRIBUTING.md`](CONTRIBUTING.md) (in Spanish) and [`docs/arquitectura.md`](docs/arquitectura.md) for the design. Commit messages are written in English.

## License

[GPL-3.0-or-later](LICENSE). Wister uses [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and OpenAI's [Whisper](https://github.com/openai/whisper) models (both MIT), [Silero VAD](https://github.com/snakers4/silero-vad) (MIT, model bundled in `crates/wister-core/assets/`) and [Tauri](https://tauri.app) (MIT/Apache-2.0).
