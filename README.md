# Accu-Chek USB Downloader (Rust)

A minimal Rust application to download and export glucose readings from Roche Accu-Chek devices over USB.

## Requirements

- Rust toolchain (stable)
- libusb (Linux) or WinUSB driver (Windows via Zadig)

## Build

cargo build --release

## Run

cargo run --release

Windows (driver)
- Install Zadig: https://zadig.akeo.ie/ and install the WinUSB driver for your Accu-Chek device.
- Run Zadig as Administrator if required.
- Run the built executable: `.\target\release\accuchek.exe`
- Debug (PowerShell): `$env:ACCUCHEK_DBG=1; .\target\release\accuchek.exe sync`

# CLI examples
```
accuchek sync    # download from device and save to DB
accuchek path    # show data/config locations
accuchek help    # show CLI help
```

## Data and Config
Data directory is OS-specific (use `accuchek path` to view). Key files:
- `accuchek.db` — SQLite database containing readings (mg/dL and mmol/L), notes, tags
- `config.txt` — configuration (device whitelist, optional custom DB path)

## PDF Export
Exported reports contain statistics and charts in the chosen unit (mg/dL or mmol/L).

## License
See LICENSE for license terms.
