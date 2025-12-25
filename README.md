# Accu-Chek USB Downloader (Rust Port)

A cross-platform Rust application for downloading and managing blood glucose data from Roche Accu-Chek devices via USB.

## Features

- **Cross-platform**: Works on Windows, Linux, and macOS (probably. It's untested.)
- **GUI Application**: Modern graphical interface with dashboard, readings list, and charts
- **SQLite Database**: Persistent storage with notes and tags for each reading
- **PDF Export**: Generate professional reports with statistics, charts, and data tables
- **IEEE 11073 Protocol**: Uses the Personal Health Device (PHD) protocol
- **CLI Mode**: Command-line interface for scripting and automation

## Screenshots

The application includes:
- **Dashboard**: Overview with statistics, time-in-range, and recent readings
- **Readings**: Searchable list with note/tag editing for each reading
- **Charts**: Interactive glucose trend visualization

## Supported Devices

The following devices are supported (add more in config file):

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
cargo build --release
```

## Usage

### GUI Mode (Default)

Simply run the application to launch the graphical interface:

```bash
# Launch GUI
cargo run --release

# Or run the built executable
./target/release/accuchek      # Linux/macOS
.\target\release\accuchek.exe  # Windows
```

### CLI Mode

```bash
# Download from device and save to database
accuchek sync

# Show data file locations
accuchek path

# Show help
accuchek help

# With debug output
ACCUCHEK_DBG=1 accuchek sync
```

## Data Storage

Data is stored in OS-appropriate locations:

| OS | Data Directory |
|----|----------------|
| Windows | `C:\Users\<user>\AppData\Roaming\accuchek\` |
| Linux | `~/.local/share/accuchek/` |
| macOS | `~/Library/Application Support/accuchek/` |

Files stored:
- `accuchek.db` - SQLite database with all readings, notes, and tags
- `config.txt` - Configuration file (auto-created on first run)

Run `accuchek path` to see exact locations on your system.

## Configuration

A default `config.txt` is created automatically in the data directory on first run.

```
# Device whitelist: vendor_0xXXXX_device_0xYYYY 1
vendor_0x173a_device_0x21d5 1  # Accu-Chek model 929
vendor_0x173a_device_0x21d7 1  # Accu-Chek model
vendor_0x173a_device_0x21d8 1  # Relion Platinum model 982

# Optional: Custom database path
# database_path C:\path\to\custom\accuchek.db
```

## PDF Export

Export your data as a PDF report from the GUI (Dashboard tab → Export PDF). Reports include:
- Summary statistics (average, min, max, reading count)
- Time in range analysis with visual bars
- Reading distribution breakdown
- Glucose trend chart with threshold lines
- Complete data table with notes and tags

## Database Schema

Readings are stored in SQLite with the following fields:
- `id` - Unique identifier
- `epoch` - Unix timestamp
- `timestamp` - Human-readable date/time
- `mg_dl` - Glucose in mg/dL
- `mmol_l` - Glucose in mmol/L
- `note` - User notes (editable in GUI)
- `tags` - Comma-separated tags (e.g., "fasting,before_meal")
- `imported_at` - When the reading was imported

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
$env:ACCUCHEK_DBG=1; .\accuchek.exe sync

# Linux/macOS
ACCUCHEK_DBG=1 ./accuchek sync
```

## Dependencies

- `rusb` - USB communication
- `rusqlite` - SQLite database
- `eframe`/`egui` - GUI framework
- `egui_plot` - Chart visualization
- `printpdf` - PDF generation
- `rfd` - Native file dialogs
- `dirs` - OS-specific directories
- `chrono` - Date/time handling
- `serde`/`serde_json` - JSON serialization

## License

MIT License - Same as the original C++ project.

## Credits

Ported from the original C++ implementation. Protocol reverse-engineered from the [Tidepool uploader](https://github.com/tidepool-org/uploader/tree/master/lib/drivers/roche).
