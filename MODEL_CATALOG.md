# Model catalog

`model-catalog.json` is the source of truth for downloadable Whisper models.
Both the app and `scripts/download-model.ps1` read this file.

Each entry contains:

- `id`: model id used by the app and model filename, for example `large-v3-turbo`.
- `download_url`: primary model download URL shown in Settings.
- `mirror_urls`: optional fallback URLs used when the primary source fails.
- `sha1`: SHA1 checksum published by the model source.
- `size`: estimated disk and VRAM sizes shown by the UI.

To update a model link or checksum, edit `model-catalog.json` instead of changing
Rust or PowerShell code.

At runtime, the app first looks for `model-catalog.json` next to the executable,
then in the current working directory. If both are missing or invalid, the
embedded catalog from the repository is used.
