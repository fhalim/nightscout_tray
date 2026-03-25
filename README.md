# NightScout Tray

Small Rust KDE system tray application for showing the latest NightScout CGM reading as a numeric tray icon.

## Dependencies

- Rust toolchain with Cargo
- KDE Plasma or another desktop that supports StatusNotifierItem tray icons
- Network access to a NightScout server

Rust crate dependencies are declared in `Cargo.toml`:

- `eframe` and `egui` for the settings and error windows
- `ksni` for the tray icon and menu integration
- `reqwest` for blocking HTTP requests to NightScout
- `serde` and `toml` for configuration parsing and serialization
- `directories` for resolving the XDG config location

## Behavior

- Starts in the system tray and renders the current glucose value directly into the tray icon
- Loads configuration from the XDG config path `~/.config/nightscout_tray/config.toml`
- If no config exists yet, defaults to:
  - `nightscout_url = "http://localhost:1337"`
  - `api_token = "mysecrettoken"`
  - `refresh_minutes = 5`
- Polls `BASE_URL/api/v1/entries.json?token=TOKEN`
- Stores up to the latest 10 NightScout entries in memory
- Displays the `sgv` value from the first returned entry in the tray icon
- Supports tray actions for refresh, settings, and quit
- Opens a small Rust-native settings window and saves the edited TOML on success

## Usage

Run the app:

```bash
cargo run
```

Open the tray menu and choose `Settings...` to edit the config.

The saved config file at `~/.config/nightscout_tray/config.toml` looks like:

```toml
nightscout_url = "http://localhost:1337"
api_token = "mysecrettoken"
refresh_minutes = 5
launch_on_startup = false

[thresholds]
low_warn = 70
low_critical = 50
high_warn = 200
high_critical = 300
```

Build a release binary:

```bash
cargo build --release
```

## Releases

- GitHub Actions builds Linux release artifacts on tag pushes like `v0.1.0`
- The release workflow publishes:
  - a `.deb` package for Debian/Ubuntu-style systems
  - a `.tar.gz` archive containing the binary, desktop entry, and README
- You can also trigger the workflow manually with `workflow_dispatch` to generate artifacts without creating a tagged release

## CI Builds

- `.github/workflows/build.yml` builds and tests the project on pushes to `master`, pull requests, and manual runs
- The build workflow uploads a Linux `.tar.gz` artifact for each run so you can download a ready-to-test binary bundle from GitHub Actions

## Notes

- Right now the app uses a simple controller thread and blocking requests.
- Poll failures are logged to stderr and the tray stays alive.
- The app currently focuses on the latest SGV value; trend and alert behavior can be added later.
