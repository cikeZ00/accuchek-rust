# Accu-Chek USB Downloader (Rust Port)

A cross-platform Rust implementation for downloading blood glucose data from Roche Accu-Chek devices via USB.

## Features

- Cross-platform: Works on Windows and Linux
- Uses the IEEE 11073 Personal Health Device (PHD) protocol
- Outputs readings as JSON
- Debug mode for troubleshooting

## Supported Devices

The following devices are supported (add more in `config.txt`):

- Roche Accu-Chek Guide (vendor: 0x173a, product: 0x21d5)
- Roche Accu-Chek (vendor: 0x173a, product: 0x21d7)
- Roche Relion Platinum (vendor: 0x173a, product: 0x21d8)

## Prerequisites

### Windows

1. Install [Zadig](https://zadig.akeo.ie/) to install the WinUSB driver for your Accu-Chek device
2. Connect your Accu-Chek device
3. Run Zadig, select your Accu-Chek device, and install the WinUSB driver

### Linux

1. Run as root (or configure udev rules for non-root access)
2. libusb-1.0 must be installed:
   ```bash
   # Debian/Ubuntu
   sudo apt-get install libusb-1.0-0-dev
   
   # Fedora
   sudo dnf install libusb1-devel
   
   # Arch
   sudo pacman -S libusb
   ```

## Building

```bash
cd RustPort
cargo build --release
```

## Usage

```bash
# Basic usage (outputs JSON to stdout)
cargo run --release

# With debug output
ACCUCHEK_DBG=1 cargo run --release

# Select specific device (if multiple connected)
cargo run --release -- 0
```

## Configuration

Create a `config.txt` file in the working directory with whitelisted devices:

```
vendor_0x173a_device_0x21d5 1 # Roche Accu-Chek Guide
vendor_0x173a_device_0x21d7 1 # Another model
vendor_0x173a_device_0x21d8 1 # Roche Relion Platinum
```

## Output Format

```json
[
  {
    "id": 0,
    "epoch": 1735142400,
    "timestamp": "2024/12/25 12:00",
    "mg/dL": 95,
    "mmol/L": 5.277778
  }
]
```

## Troubleshooting

### Windows

- **Device not found**: Make sure the WinUSB driver is installed via Zadig
- **Access denied**: Run as Administrator if needed

### Linux

- **Permission denied**: Run with `sudo` or configure udev rules
- **Device busy**: Another program may be using the device

### Debug Mode

Set the `ACCUCHEK_DBG` environment variable to see detailed protocol traces:

```bash
# Windows PowerShell
$env:ACCUCHEK_DBG=1; cargo run --release

# Linux/macOS
ACCUCHEK_DBG=1 cargo run --release
```

## License

Same as the original C++ project.

## Credits

Ported from the original C++ implementation. Protocol reverse-engineered from the [Tidepool uploader](https://github.com/tidepool-org/uploader/tree/master/lib/drivers/roche).
