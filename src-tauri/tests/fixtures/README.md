# Test fixtures

Fixtures are recorded from the OPPO CPH2239 (Android 11 / SDK 30, arm64-v8a) unless a file says otherwise. Host ADB: 1.0.41, platform-tools 37.0.1-15733141, macOS 15.7 arm64. Captures dated 2026-10-06.

Serials are replaced with `TESTSERIAL0001`; private LAN addresses are replaced with TEST-NET `192.0.2.0/24`; Wi-Fi MACs and hardware serial values are omitted or redacted. The Hermes gateway excerpt keeps timestamps and levels but replaces every logger/message with `<redacted>`.

The wireless fixture with two rows is two ADB transports for the same physical phone, not two phones. `devices_none.txt` was captured from an isolated temporary ADB server with no attached devices. `connect_refused_localhost.txt` is a host-local refused connection against `127.0.0.1:1`, not a phone result. `connect_already.txt` records the live device's already-connected response with the address redacted. Existing `devices_wireless_mdns.txt`, `mdns_services.txt`, and `track_frames.bin` are prior parser fixtures. Phone-dependent cases still missing include USB, unauthorized/offline, authentication/timeout/pairing failures, QR-pair discovery, Android-version variants, and boot/provisioning behavior. Do not infer those cases from synthetic fixtures.
