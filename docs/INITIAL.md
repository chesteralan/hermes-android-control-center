Build: Hermes Android Control Center

Build a macOS desktop application for remotely managing a Hermes Agent running inside Termux on an Android phone.

The goal is to replace the need for scrcpy for day-to-day Hermes management. The application should provide a native-feeling desktop control panel for:

* Connecting to an Android device over Wireless ADB
* Detecting the Android device
* Starting/stopping/restarting Hermes Agent
* Executing commands inside Termux
* Streaming Hermes logs to the Mac in real time
* Viewing Hermes status and basic system information
* Sending commands to Hermes
* Managing the Android/Termux connection

Do not implement Android screen mirroring. scrcpy is not required.

⸻

1. Technology Stack

Use:

Desktop

* Tauri 2
* Rust backend
* React
* TypeScript
* Vite
* Tailwind CSS
* Modern component architecture

Android

* Termux
* Hermes Agent
* Wireless ADB

The application should be designed so that the Android-side component can eventually become a small dedicated Hermes control service running inside Termux.

⸻

2. High-Level Architecture

Use this architecture:

┌────────────────────────────── macOS ──────────────────────────────┐
│                                                                  │
│  Hermes Android Control Center                                   │
│                                                                  │
│  React + TypeScript                                              │
│             │                                                    │
│             ▼                                                    │
│       Tauri IPC                                                  │
│             │                                                    │
│             ▼                                                    │
│        Rust Backend                                              │
│        ┌───────────────┐                                         │
│        │ ADB Manager   │                                         │
│        │ Device Manager│                                         │
│        │ Command Exec  │                                         │
│        │ Log Streamer  │                                         │
│        │ Hermes API    │                                         │
│        └───────────────┘                                         │
│             │                                                    │
└─────────────┼────────────────────────────────────────────────────┘
              │
              │ Wireless ADB / Wi-Fi
              ▼
┌──────────────────────────── Android ──────────────────────────────┐
│                                                                  │
│  Termux                                                          │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Hermes Agent                                               │  │
│  │                                                            │  │
│  │ Python process                                             │  │
│  │ Gateway                                                    │  │
│  │ Logs                                                       │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘

The system should support two communication mechanisms:

ADB

Use ADB for:

* Device discovery
* Initial connection
* Bootstrapping the Termux service
* Starting/stopping Hermes
* Running fallback commands
* Emergency/debug access

Hermes Control API

Design the architecture to support a lightweight HTTP/WebSocket service inside Termux.

Use it for:

* Status
* Commands
* Real-time logs
* Process management
* Future Hermes-specific functionality

Do not require this API to be fully implemented in the first milestone if that would unnecessarily block development. Build the Rust abstraction so it can be added cleanly.

⸻

3. Core Features

Device Connection

The application should detect Android devices available through ADB.

Display:

Android Device
● Connected
Device: Pixel 8
Android: 15
IP: 192.168.1.25
ADB: 192.168.1.25:xxxxx

Support:

adb devices

and Wireless ADB connections.

Allow the user to manually enter:

IP address
Port

and connect using:

adb connect <ip>:<port>

Also support:

adb disconnect

⸻

4. Device Status

Display:

* Connection status
* Device model
* Android version
* Android SDK version
* Device serial
* IP address
* Battery percentage
* Charging state
* Available storage
* RAM information
* CPU information where available

Example:

Device
────────────────────────
● Connected
Pixel 8
Android 15
192.168.1.25:42567
Battery       82%
Storage       48 GB free
Memory        3.2 GB / 8 GB

System information should be retrieved through ADB initially.

⸻

5. Hermes Status

Provide a dedicated Hermes status card.

Example:

Hermes Agent
● Running
PID: 18432
Uptime: 2h 31m
Python: 3.13.x
Gateway
● Running
Telegram
● Connected

The exact information should depend on what can be reliably retrieved from the installed Hermes Agent.

Do not fabricate status information.

If something cannot be determined:

Unknown

or:

Unavailable

⸻

6. Hermes Controls

Provide buttons:

[ Start Hermes ]
[ Restart Hermes ]
[ Stop Hermes ]

Before destructive actions such as stopping Hermes, show a confirmation dialog.

The commands should be configurable rather than hard-coded throughout the UI.

For example, define a Rust configuration structure:

struct HermesConfig {
    start_command: String,
    stop_command: String,
    restart_command: String,
    status_command: String,
}

The exact Termux/Hermes commands should be configurable.

⸻

7. Command Terminal

Create a terminal-like command interface.

Example:

┌────────────────────────────────────────────────────────────┐
│ Command                                                    │
├────────────────────────────────────────────────────────────┤
│ $ hermes status                                            │
│                                                            │
│ Hermes Agent: running                                      │
│ Gateway: running                                           │
│                                                            │
│ $                                                        │
└────────────────────────────────────────────────────────────┘

Allow commands to be executed through ADB.

Example:

adb shell "<command>"

Display:

* stdout
* stderr
* exit code
* execution duration

Use streaming output for long-running commands.

Do not block the UI while commands are running.

⸻

8. Real-Time Logs

Create a dedicated log viewer.

Example:

Logs
─────────────────────────────────────────────────────────────
13:42:01  INFO   Hermes gateway started
13:42:02  INFO   Telegram connection established
13:42:05  INFO   Agent initialized
13:42:10  INFO   Waiting for messages
13:43:17  INFO   Received message
13:43:18  INFO   Executing task
13:43:22  INFO   Task completed

Requirements:

* Real-time streaming
* Auto-scroll
* Pause scrolling
* Clear logs
* Search logs
* Filter by log level
* Copy selected logs
* Export logs to a file

Support:

INFO
WARN
ERROR
DEBUG

where log levels can be determined.

If Hermes outputs unstructured logs, preserve the original text.

⸻

9. Log Streaming Architecture

Do not continuously execute a new ADB command for every log line.

Instead, implement a persistent stream.

Initially support something equivalent to:

adb shell "<hermes log command>"

with stdout streamed to the Rust backend.

Later allow:

Termux Hermes Control API
        │
        │ WebSocket
        ▼
Rust backend
        │
        ▼
React log viewer

Create an abstraction such as:

trait LogSource {
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn stream(&self) -> Result<LogStream>;
}

This should allow ADB logs and WebSocket logs to use the same UI.

⸻

10. Rust ADB Layer

Create a dedicated ADB module.

Example structure:

src-tauri/
├── src/
│   ├── main.rs
│   ├── adb/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   ├── device.rs
│   │   ├── command.rs
│   │   └── logs.rs
│   │
│   ├── hermes/
│   │   ├── mod.rs
│   │   ├── manager.rs
│   │   ├── status.rs
│   │   └── api.rs
│   │
│   ├── commands/
│   │   ├── device.rs
│   │   ├── hermes.rs
│   │   └── terminal.rs
│   │
│   └── state.rs

Do not scatter Command::new("adb") calls throughout the application.

Centralize ADB functionality.

⸻

11. ADB Binary

Initially assume adb is installed and available in PATH.

Provide a settings page where the user can specify a custom ADB path:

ADB Path
[ /opt/homebrew/bin/adb                  ]
[ Detect ]

Automatically detect common locations such as:

/opt/homebrew/bin/adb
/usr/local/bin/adb

and the Android SDK platform-tools directory.

Do not bundle ADB in the first version.

⸻

12. Connection Monitoring

Continuously monitor the Android connection.

If the connection drops:

● Disconnected

Attempt reconnection using the previously known device address.

Use a reasonable retry strategy with backoff.

Do not aggressively reconnect every few milliseconds.

Example:

1s
2s
5s
10s
30s

Stop retrying after a reasonable limit and allow the user to manually retry.

⸻

13. Multiple Android Devices

Design the backend so multiple devices are possible in the future.

Do not globally assume a single device.

Create a device abstraction:

struct AndroidDevice {
    serial: String,
    model: Option<String>,
    ip_address: Option<String>,
    state: DeviceState,
}

The first UI may focus on one device, but the architecture should not prevent multiple devices.

⸻

14. UI Layout

Use a clean developer-tool style interface.

Suggested layout:

┌─────────────────────────────────────────────────────────────┐
│ Hermes Control Center                              Settings │
├──────────────┬──────────────────────────────────────────────┤
│              │                                              │
│ Dashboard    │  Android Device                              │
│              │  ● Connected                                 │
│ Device       │                                              │
│              │  Pixel 8                                     │
│ Hermes       │  Android 15                                  │
│              │  192.168.1.25                                │
│ Terminal     │                                              │
│              │  ┌──────────────┐ ┌──────────────┐           │
│ Logs         │  │ Hermes       │ │ Gateway      │           │
│              │  │ ● Running    │ │ ● Running    │           │
│ Settings     │  └──────────────┘ └──────────────┘           │
│              │                                              │
│              │  [ Restart ] [ Stop ]                        │
│              │                                              │
│              │  Recent Logs                                 │
│              │  ┌────────────────────────────────────────┐  │
│              │  │ 13:42:01 Hermes started                │  │
│              │  │ 13:42:02 Gateway connected             │  │
│              │  │ 13:42:05 Agent ready                  │  │
│              │  └────────────────────────────────────────┘  │
│              │                                              │
└──────────────┴──────────────────────────────────────────────┘

Use dark mode as the default.

Make the interface feel like a professional developer tool rather than a generic admin dashboard.

⸻

15. Tauri Commands

Expose strongly typed Tauri commands.

For example:

#[tauri::command]
async fn list_devices() -> Result<Vec<AndroidDevice>, AppError>
#[tauri::command]
async fn connect_device(
    serial: String
) -> Result<AndroidDevice, AppError>
#[tauri::command]
async fn execute_command(
    serial: String,
    command: String
) -> Result<CommandResult, AppError>
#[tauri::command]
async fn get_hermes_status(
    serial: String
) -> Result<HermesStatus, AppError>
#[tauri::command]
async fn restart_hermes(
    serial: String
) -> Result<(), AppError>

For streaming logs, use Tauri events or an equivalent event-stream mechanism rather than repeatedly polling.

⸻

16. Error Handling

Errors must be understandable.

Instead of:

Exit code 1

show:

Unable to connect to Android device.
ADB returned:
device offline

Handle:

* ADB not installed
* ADB not found
* Device offline
* Device unauthorized
* Connection refused
* Wireless debugging disabled
* Termux unavailable
* Hermes process not found
* Command failed
* Network timeout

Never silently swallow errors.

Provide technical details behind an expandable “Details” section.

⸻

17. Security

The command terminal is powerful.

Treat commands as potentially destructive.

Do not execute arbitrary commands automatically.

For normal terminal commands:

$ command

the user explicitly submits them.

For predefined actions such as:

Restart Hermes
Stop Hermes

use known commands from configuration.

Do not expose the Hermes control API publicly on the Internet.

If implementing the Termux HTTP/WebSocket server, bind it to:

127.0.0.1

where possible, or otherwise require authentication/token-based access when it must listen on the LAN.

⸻

18. Configuration

Persist application settings locally.

Example:

ADB path
Last connected device
Hermes start command
Hermes stop command
Hermes restart command
Hermes status command
Log command
API port
Auto-connect
Auto-start log streaming

Use a proper configuration/state mechanism rather than writing arbitrary files throughout the project.

⸻

19. Logging

The Rust application itself should have structured logs.

Use a Rust logging/tracing library.

Provide:

Debug
Info
Warn
Error

The application should not expose sensitive data unnecessarily in logs.

⸻

20. Testing

Add tests for:

Rust

* ADB command construction
* Device parsing
* ADB output parsing
* Device state handling
* Connection retry logic
* Hermes status parsing
* Error handling

React

* Device status rendering
* Connection states
* Log viewer
* Terminal
* Error states

Do not require a physical Android device for unit tests.

Create mock ADB responses.

⸻

21. Development Commands

The project should support:

npm install
npm run dev
npm run build
npm run tauri dev
npm run tauri build

Use the package manager already established by the repository if one exists.

Do not introduce another package manager unnecessarily.

⸻

22. Development Strategy

Implement this incrementally.

Phase 1 — Desktop Shell

Create:

* Tauri app
* React UI
* Navigation
* Dark developer-tool UI
* Settings
* Rust backend structure

Phase 2 — ADB

Implement:

* ADB detection
* Device listing
* Wireless ADB connection
* Device information
* Connection status

At this point the application should be able to replace basic:

adb devices
adb connect
adb shell

operations.

Phase 3 — Hermes

Implement:

* Hermes process detection
* Hermes status
* Start
* Stop
* Restart
* Configurable commands

Phase 4 — Terminal

Implement:

* Interactive command execution
* stdout streaming
* stderr streaming
* exit codes
* command history

Phase 5 — Logs

Implement:

* Persistent log stream
* Real-time UI
* Search
* Filtering
* Pause
* Clear
* Export

Phase 6 — Hermes Control API

Add the optional Termux-side service architecture.

Implement:

GET /status
POST /command
POST /start
POST /stop
POST /restart
WebSocket /logs

Use the same frontend interfaces regardless of whether the data comes from ADB or the API.

⸻

23. Important Design Principle

Create interfaces around the communication layer.

For example:

trait DeviceTransport {
    async fn execute(&self, command: &str) -> Result<CommandResult>;
    async fn stream(&self, command: &str) -> Result<CommandStream>;
}

Then implement:

AdbTransport
ApiTransport

This means the UI does not need to know whether it is communicating through ADB or the Hermes API.

Eventually:

                 ┌── AdbTransport
Hermes Manager ──┤
                 └── ApiTransport

This is an important architectural requirement.

⸻

24. First Milestone

Do not attempt to implement everything at once.

The first working milestone should be:

1. Launch the Tauri application.
2. Detect adb.
3. Run adb devices.
4. Display connected Android devices.
5. Select a device.
6. Connect to it.
7. Display device information.
8. Execute:

adb shell <command>

9. Display stdout/stderr.
10. Provide a basic terminal.
11. Provide a basic live log viewer.

Only after this works should Hermes-specific functionality be added.

⸻

25. Deliverables

Produce:

* Complete source code
* Tauri configuration
* Rust backend
* React frontend
* TypeScript types
* ADB abstraction
* Device manager
* Hermes manager
* Terminal
* Log viewer
* Settings
* Tests
* README

The README must document:

1. Prerequisites
2. Installing ADB
3. Enabling Android Developer Options
4. Enabling Wireless Debugging
5. Pairing the Android device
6. Connecting the device
7. Configuring Termux
8. Configuring Hermes
9. Running the application
10. Troubleshooting ADB connection issues

⸻

26. Development Rules

Before writing code:

1. Inspect the existing repository.
2. Identify the existing package manager.
3. Identify existing conventions.
4. Do not overwrite existing functionality unnecessarily.
5. Reuse existing dependencies when possible.
6. Keep Rust modules small and focused.
7. Keep React components modular.
8. Use TypeScript types instead of any.
9. Use proper error types in Rust.
10. Do not hard-code device IP addresses.
11. Do not hard-code Hermes process IDs.
12. Do not assume a specific Android device model.
13. Do not assume a specific Hermes installation path.
14. Make commands configurable.
15. Keep ADB and Hermes communication behind abstractions.
16. Do not implement screen mirroring.
17. Do not add unnecessary authentication infrastructure for local-only ADB communication.
18. Prioritize a working Phase 1/2 implementation before adding advanced functionality.

Start by inspecting the repository and then implement Phase 1 and Phase 2. After completing each phase, run the relevant tests/build checks and fix errors before proceeding.