# Tablet2Screen Roadmap

## 0. Project Positioning

Tablet2Screen is a Windows-to-HarmonyOS/Android tablet display streaming system.

The first practical target is to use a Huawei MatePad Air 2024 running HarmonyOS 6 as a secondary display receiver for a Windows 11 PC.

This project does not aim to implement a virtual display driver. It assumes that the Windows system already has an available physical or virtual display. For the current prototype, the virtual display may be provided by an external tool such as Parsec VDisplay.

The project focuses on:

- Capturing an existing Windows display.
- Encoding the captured frames.
- Transmitting the encoded stream to the tablet.
- Decoding and rendering the stream fullscreen on the tablet.

The project explicitly does not focus on:

- Creating a virtual display driver.
- Installing or managing a virtual display driver.
- Touch input forwarding.
- Stylus input forwarding.
- Keyboard or mouse input forwarding.
- Audio streaming.
- Multi-tablet support.
- Public internet remote access.
- Cloud service.
- Account system.
- File transfer.
- Full remote desktop control.

The current development principle is:

- Build a working prototype first.
- Use reliable development/debug transport first.
- Keep the transport layer abstract so that HDC, Wi-Fi, and future AOA transport can share the same upper-level protocol.
- Revisit formal USB transport only after the higher-level screen streaming pipeline works.

---

## 1. Current Strategic Decision

### 1.1 Main Transport for Prototype

The current prototype should use HDC port forwarding as the main transport.

Reason:

- AOA control mode has been proven, but AOA data transfer is still blocked.
- HDC provides reliable socket-style communication over USB.
- HDC allows us to focus on screen capture, encoding, protocol design, decoding, and rendering first.
- The project should not be blocked by Windows USB driver binding, libusb interface claiming, or HarmonyOS `accessoryFd` read/write issues.

### 1.2 AOA Status

AOA is not discarded. It is paused.

Already verified:

- Windows can send AOA control requests.
- MatePad Air 2024 can switch from Huawei USB mode `12d1:1101` to Google AOA mode `18d1:2d00`.
- HarmonyOS native `usbManager` can detect the accessory.
- `requestAccessoryRight(...)` succeeds.
- `openAccessory(...)` succeeds.
- The returned `USBAccessoryHandle` contains `accessoryFd`, such as `{"accessoryFd": 32}`.

Still not verified:

- Whether `accessoryFd` can be reliably read/written from ArkTS.
- Whether `accessoryFd` can be reliably read/written from Native C++/NAPI.
- Whether Windows can reliably claim and use the AOA bulk interface.
- Whether AOA throughput is sufficient for high-resolution video streaming.

Current decision:

- HDC becomes the prototype transport.
- AOA becomes a future formal USB transport candidate.
- The upper-level protocol must be designed so that AOA can replace HDC later without rewriting the capture/encode/decode/render pipeline.

---

## 2. Target Hardware and Platforms

### 2.1 Current Primary Hardware

Windows Host:

- Windows 11 laptop.
- Machine 1: AMD CPU + NVIDIA RTX 4060, ASUS TUF Gaming A15/F15 class device.
- Machine 2: Intel CPU + Intel integrated GPU, Lenovo ThinkBook 14 2024.

Tablet Client:

- Huawei MatePad Air 2024.
- HarmonyOS 6.
- Native resolution target: 2800 x 1840.
- High refresh screen, but project target is 30 FPS minimum and 60 FPS preferred.

### 2.2 Initial Platform Scope

Host:

- Windows 11 only.

Client:

- HarmonyOS 6 first.
- Huawei MatePad Air 2024 first.

Future scope:

- More Huawei tablets.
- Android 12+ tablets.
- Wi-Fi transport.
- Possibly formal AOA USB transport.

Out of scope for now:

- iPadOS.
- macOS host.
- Linux host.
- Multi-client support.
- Public internet streaming.

---

## 3. Technical Stack

### 3.1 Windows Host Stack

Preferred stack:

- UI shell: Tauri.
- Backend: Rust.
- Early probes: Python is allowed only for quick experiments.
- Final host implementation should avoid Python.

Windows modules:

- Display enumeration:
  - Enumerate physical and virtual displays.
  - Show display name, resolution, refresh rate, and whether it is likely a virtual display.

- Screen capture:
  - Prefer Windows Graphics Capture or DXGI Desktop Duplication.
  - Capture a selected display.
  - Mouse cursor must be visible in the captured output.

- Encoder:
  - Hardware encoding required.
  - NVIDIA machine: prioritize NVENC.
  - Intel machine: use Intel Quick Sync / Media Foundation where possible.
  - Codec priority:
    - H.265/HEVC preferred.
    - H.264 fallback required later.
  - Initial prototype may support only one codec if necessary.

- Transport:
  - HDC transport first.
  - Wi-Fi transport later.
  - AOA transport paused but reserved.
  - Transport abstraction required.

- Packaging:
  - Early stage: executable is acceptable.
  - Later stage: Windows installer.

### 3.2 HarmonyOS Client Stack

Preferred stack:

- HarmonyOS native application.
- ArkTS / ArkUI for UI.
- Native C++/NAPI allowed for performance-critical code.
- Hardware decoder required.
- Fullscreen rendering required.
- Landscape mode first.
- Screen should remain awake during display mode.

Client modules:

- Connection manager:
  - HDC socket connection first.
  - Wi-Fi connection later.
  - AOA connection later if revived.

- Decoder:
  - Hardware H.265 first.
  - H.264 fallback later.

- Renderer:
  - Fullscreen display.
  - Prefer 1:1 pixel display when virtual display resolution matches tablet native resolution.
  - Fallback to aspect-fit scaling.
  - No touch control overlay in display mode.

- UI:
  - Minimal setup page.
  - Connection status.
  - Error logs during prototype.
  - No in-display control bar for MVP.
  - The user can exit using native system gestures.

### 3.3 Transport Layer Design

All transports must implement the same conceptual interface:

    Transport.connect()
    Transport.close()
    Transport.sendControl(message)
    Transport.recvControl()
    Transport.sendVideoPacket(packet)
    Transport.recvVideoPacket()

Concrete transport implementations:

    HdcTransport
    WifiTransport
    AoaTransport

The upper-level Tablet2Screen protocol must not depend on HDC-specific behavior.

---

## 4. Product and Functional Boundaries

### 4.1 Must Support in Prototype

- Windows 11 host.
- HarmonyOS 6 tablet client.
- Existing display capture.
- External virtual display support through tools such as Parsec VDisplay.
- USB-connected prototype transport through HDC.
- Fullscreen tablet display.
- Landscape mode.
- Basic status logs.
- Basic error messages.
- Manual connection flow.
- One Windows host and one tablet client.
- Mouse cursor visible in the streamed image.
- H.265 preferred.

### 4.2 Not Required in Current Prototype

- Touch input forwarding.
- Stylus input forwarding.
- Keyboard input forwarding.
- Mouse input forwarding.
- Audio.
- Multiple tablets.
- Multiple hosts.
- Cloud service.
- Account system.
- Public internet remote access.
- Virtual display driver development.
- Automatic virtual display driver installation.
- Fully polished UI.
- Automatic discovery.
- QR pairing.
- Encrypted Wi-Fi transport.
- Formal AOA data transport.

### 4.3 Virtual Display Boundary

The project does not create the virtual display.

Expected workflow:

- The user creates a virtual display using an external solution such as Parsec VDisplay.
- Windows sees the virtual display as a normal display.
- Tablet2Screen captures that display and streams it to the tablet.

The project may later provide documentation for virtual display setup, but it should not integrate virtual display driver installation in the current phase.

### 4.4 Display Quality Boundary

Target display behavior:

- Prefer tablet native resolution, 2800 x 1840 on MatePad Air 2024.
- MVP accepts native resolution at 30 FPS.
- Preferred target is native resolution at 60 FPS.
- If native resolution is too heavy, fallback modes are acceptable:
  - 1920 x 1200 at 60 FPS.
  - 1920 x 1080 at 60 FPS.
  - Native resolution at 30 FPS with higher compression.

Quality priority:

- Text clarity first.
- Web pages and documents first.
- Video playback should be acceptable if possible.
- Gaming-level latency is not required.
- Drawing-tablet-level latency is not required.

---

## 5. Development Roadmap

## Phase 0: Completed USB Exploration

### Goal

Explore whether formal native HarmonyOS AOA can become the future USB transport.

### Status

Completed enough for current decision-making.

### Results

Completed:

- Windows AOA control sequence works.
- MatePad re-enumerates as AOA device `18d1:2d00`.
- HarmonyOS native `usbManager` can list and open the accessory.
- `USBAccessoryHandle` exposes `accessoryFd`.

Blocked:

- AOA data transfer not proven.
- Windows PyUSB/libusb interface claiming unreliable.
- HarmonyOS `accessoryFd` read/write unreliable.

### Decision

Pause AOA work.

AOA should be revisited only after:

- HDC ping/pong works.
- HDC throughput is known.
- HDC video streaming works.
- HarmonyOS decoding and rendering are proven.

---

## Phase 1: HDC Ping/Pong Probe

### Goal

Prove reliable socket-style communication between Windows and HarmonyOS over USB using HDC port forwarding.

### Transport Direction

Recommended initial direction:

    HarmonyOS App:
        TCP server on 127.0.0.1:7000

    HDC:
        hdc fport tcp:17000 tcp:7000

    Windows:
        TCP client connects to 127.0.0.1:17000

### Required HDC Commands

Run on Windows:

    hdc list targets
    hdc fport tcp:17000 tcp:7000
    hdc fport ls

### HarmonyOS App Requirements

Add a TCP server probe page:

- Button: Start TCP Server.
- Button: Stop TCP Server.
- Button: Clear Log.
- Listen on `127.0.0.1:7000`.
- Accept one connection.
- Read bytes.
- Print received text and hex.
- Respond with `pong-from-harmony`.

### Windows Probe Requirements

Create a minimal Python client:

    connect 127.0.0.1:17000
    send b"hello-from-windows"
    receive response
    print response

### Success Criteria

- Windows sends `hello-from-windows`.
- HarmonyOS app logs the received message.
- HarmonyOS sends `pong-from-harmony`.
- Windows receives and prints `pong-from-harmony`.

### Failure Cases to Handle

- `hdc list targets` cannot see device.
- `fport` fails.
- HarmonyOS server cannot bind port.
- Windows cannot connect to local forwarded port.
- Data sent but no response.
- App blocks UI thread.

---

## Phase 2: HDC Throughput Probe

### Goal

Measure whether HDC transport can carry enough data for H.265 video streaming.

### Test Design

Windows sends fixed-size binary blocks to the HarmonyOS app.

Recommended block size:

    64 KB per block

Recommended test durations:

    10 seconds
    30 seconds
    5 minutes

Recommended target rates:

    10 Mbps
    30 Mbps
    60 Mbps
    100 Mbps
    150 Mbps

HarmonyOS app counts received bytes and reports:

- Bytes per second.
- Mbps.
- Total bytes.
- Connection duration.
- Error count.
- Whether UI remains responsive.

### Success Criteria

Minimum useful results:

- 30 Mbps stable: enough for early 1080p prototype.
- 60-100 Mbps stable: useful for 2800 x 1840 at 30 FPS with H.265.
- 120-200 Mbps stable: useful for native resolution 60 FPS experiments.

### Output Needed

A small report:

- Average throughput.
- Peak throughput.
- Stability.
- CPU load if observable.
- Heat level subjective note.
- Whether the connection drops.

---

## Phase 3: HarmonyOS Decoder Probe

### Goal

Verify that the MatePad can decode and render H.265 video at target resolutions.

### Input

Use local test files first. Do not involve Windows live capture yet.

Recommended test files:

- 1920 x 1080, 60 FPS, H.265.
- 2800 x 1840, 30 FPS, H.265.
- 2800 x 1840, 60 FPS, H.265.
- Optional H.264 fallback samples.

### HarmonyOS Requirements

- Hardware decoding.
- Fullscreen rendering.
- Landscape mode.
- Keep screen awake.
- Print FPS / dropped frame statistics if possible.

### Success Criteria

Minimum:

- 1920 x 1080 at 60 FPS stable.
- 2800 x 1840 at 30 FPS stable.

Preferred:

- 2800 x 1840 at 60 FPS stable.

### Decision Point

If native resolution decoding is unstable:

- Use 1920 x 1200 or 1920 x 1080 for early MVP.
- Keep native resolution as optimization target.

---

## Phase 4: HDC H.265 File Stream Probe

### Goal

Stream an existing H.265 file from Windows to HarmonyOS over HDC and decode it on the tablet.

### Windows Side

- Read a local H.265 Annex B stream or fragmented H.265 stream.
- Send it through HDC socket transport.
- Do not capture screen yet.
- Do not implement complex adaptive bitrate.

### HarmonyOS Side

- Receive byte stream.
- Feed it into hardware decoder.
- Render fullscreen.
- Print logs for decoder state and connection state.

### Success Criteria

- Windows sends H.265 stream.
- Tablet decodes and displays it.
- No file is saved on the tablet.
- Stream plays with acceptable delay.
- Connection remains stable for at least 10 minutes.

### Notes

This phase proves:

- HDC transport can carry video-like data.
- HarmonyOS decoder can handle streamed input.
- The client render pipeline works.

---

## Phase 5: Windows Screen Capture and Local Encode Probe

### Goal

Capture an existing Windows display, encode it with hardware encoder, and validate the encoded output locally.

### Windows Requirements

- Enumerate displays.
- Select a display.
- Capture selected display.
- Include mouse cursor.
- Encode to H.265.
- Save short test stream or preview locally.
- Log FPS, encode latency, bitrate.

### Capture Candidates

- Windows Graphics Capture.
- DXGI Desktop Duplication.

### Encoder Candidates

For RTX 4060 machine:

- NVENC H.265 first.

For Intel machine:

- Intel Quick Sync / Media Foundation path.

### Success Criteria

- Capture selected display.
- Cursor is visible.
- H.265 encoding works.
- 1920 x 1080 or 1920 x 1200 at 60 FPS works.
- 2800 x 1840 at 30 FPS is attempted.
- Encoded output can be decoded locally.

---

## Phase 6: HDC Live Screen Streaming Prototype

### Goal

Connect the capture/encode pipeline to the HDC transport and render the live stream on the tablet.

### Full Pipeline

    Windows display
        -> screen capture
        -> hardware H.265 encoder
        -> Tablet2Screen protocol
        -> HDC socket transport
        -> HarmonyOS receiver
        -> hardware decoder
        -> fullscreen renderer

### Required Features

Windows side:

- Select display.
- Start streaming.
- Stop streaming.
- Show current resolution.
- Show current FPS.
- Show current bitrate.
- Show connection status.

HarmonyOS side:

- Connect/listen through HDC forwarded socket.
- Receive stream.
- Decode stream.
- Render fullscreen.
- Show connection status before display starts.
- Exit through system gesture.

### Success Criteria

- The tablet displays the selected Windows display.
- The virtual display from Parsec VDisplay can be selected and streamed.
- Cursor is visible.
- Documents and web pages are readable.
- 30 FPS at high resolution is usable.
- 60 FPS is attempted.
- Connection runs for at least 30 minutes.

---

## Phase 7: Tablet2Screen Protocol Stabilization

### Goal

Define a transport-independent protocol that works over HDC now and can later work over Wi-Fi or AOA.

### Message Framing

Use a binary framing format.

Suggested header:

    magic: 4 bytes
    version: 1 byte
    type: 1 byte
    flags: 2 bytes
    sequence: 4 bytes
    timestamp_us: 8 bytes
    payload_len: 4 bytes
    payload: variable length

Suggested message types:

    HELLO
    HELLO_ACK
    VIDEO_CONFIG
    VIDEO_CONFIG_ACK
    VIDEO_PACKET
    KEYFRAME_REQUEST
    STATS
    ERROR
    STOP

### Design Rules

- Do not accumulate old video frames.
- Prefer latest frame over old frame backlog.
- Allow dropping stale frames.
- Keep control messages separate from video packets if needed.
- The protocol must not depend on HDC-specific semantics.
- The protocol should support future Wi-Fi and AOA transports.

### Success Criteria

- HDC transport can be replaced without changing encoder/decoder logic.
- Basic statistics can be exchanged.
- Receiver can request a keyframe.
- Sender can stop cleanly.
- Connection errors are clearly reported.

---

## Phase 8: Usable HDC-Based MVP

### Goal

Create a usable personal prototype using HDC over USB.

### Windows App

Technology:

- Tauri UI.
- Rust backend.

Features:

- Display selection.
- Start/stop stream.
- Basic quality settings.
- Basic resolution/FPS selection.
- HDC connection instructions.
- Current bitrate/FPS display.
- Error logs.
- Optional system tray.

### HarmonyOS App

Technology:

- ArkTS/ArkUI.
- Native C++/NAPI if needed for decoder or socket performance.

Features:

- Start receiver.
- Show connection status.
- Fullscreen display.
- Landscape mode.
- Keep screen awake.
- Error logs before stream starts.

### MVP Boundaries

Required:

- One Windows host.
- One HarmonyOS tablet.
- HDC over USB.
- Existing display capture.
- External virtual display supported.
- Fullscreen rendering.
- No input forwarding.
- No audio.

Not required:

- Formal AOA transport.
- Wi-Fi transport.
- Automatic pairing.
- Auto discovery.
- Commercial-level installer.
- Multi-device support.

---

## Phase 9: Wi-Fi Transport

### Goal

Add normal local network transport after HDC video streaming works.

### Connection Model

Initial Wi-Fi version:

- Manual IP address.
- Manual port.
- One host and one tablet.
- Same LAN only.

Later Wi-Fi version:

- Device discovery.
- Pairing code.
- Whitelist.
- Encrypted control channel.
- Encrypted video channel.

### Security Boundary

Wi-Fi must not be treated as trusted by default.

Required later:

- First-time pairing.
- Device whitelist.
- Encrypted video stream if used beyond personal testing.

### Success Criteria

- Same protocol works over Wi-Fi.
- HDC transport and Wi-Fi transport share upper-level code.
- Wi-Fi performance is acceptable for document/web use.
- Clear error messages for firewall, wrong IP, and timeout.

---

## Phase 10: Revisit AOA Formal USB Transport

### Goal

Return to AOA only after the HDC-based video prototype works.

### Existing Evidence

AOA control path is proven:

- Protocol version 2.
- Re-enumeration to `18d1:2d00`.
- Native HarmonyOS accessory APIs work up to `openAccessory`.
- `accessoryFd` exists.

### Remaining Work

Windows side:

- Reliable WinUSB/libusb binding.
- Reliable interface claim.
- Bulk IN/OUT endpoint access.
- Error recovery.

HarmonyOS side:

- Reliable `accessoryFd` read/write.
- If ArkTS file APIs fail, try Native C++/NAPI.
- Throughput test.
- Long-running stability test.

### Success Criteria

- Windows sends `hello-from-windows` over AOA bulk OUT.
- HarmonyOS receives it from `accessoryFd`.
- HarmonyOS sends `pong-from-harmony`.
- Windows receives it over AOA bulk IN.
- Throughput reaches useful video rates.
- No HDC/ADB/debug mode is required.

### Decision Point

If AOA data path becomes stable:

- Implement `AoaTransport`.
- Keep the same Tablet2Screen protocol.
- Replace HDC tunnel with AOA transport for formal USB mode.

If AOA remains unstable:

- Keep HDC as developer-mode USB transport.
- Use Wi-Fi as user-friendly transport.
- Do not block the product on AOA.

---

## 6. Version Plan

### v0.0: USB and Platform Exploration

Status:

- Mostly completed.

Includes:

- HarmonyOS AOA API probe.
- Windows AOA control probe.
- AOA status report.
- Decision to pause AOA data path.

### v0.1: HDC Ping/Pong

Goal:

- Reliable socket communication over HDC.

Deliverables:

- HarmonyOS TCP server.
- Windows Python client.
- HDC forwarding instructions.
- Ping/pong success report.

### v0.2: HDC Throughput

Goal:

- Measure transport capacity.

Deliverables:

- Throughput sender.
- Throughput receiver.
- Mbps report.
- Stability report.

### v0.3: Decoder Probe

Goal:

- Prove HarmonyOS hardware decoding and fullscreen rendering.

Deliverables:

- Local H.265 playback probe.
- Resolution/FPS report.
- Thermal/stability notes.

### v0.4: HDC File Stream

Goal:

- Stream H.265 file over HDC and decode on tablet.

Deliverables:

- Windows stream sender.
- HarmonyOS stream receiver.
- Fullscreen rendering.

### v0.5: Capture + Encode Probe

Goal:

- Capture and encode Windows display.

Deliverables:

- Display enumeration.
- Screen capture.
- H.265 hardware encoding.
- Cursor visible.
- Local encoded output validation.

### v0.6: Live HDC Screen Streaming

Goal:

- End-to-end live display prototype.

Deliverables:

- Windows capture/encode/send.
- HarmonyOS receive/decode/render.
- Display selected Windows screen on tablet.

### v0.7: HDC-Based Personal MVP

Goal:

- Usable personal tool.

Deliverables:

- Tauri Windows UI.
- HarmonyOS basic UI.
- HDC setup guide.
- Virtual display setup guide.
- Basic settings and diagnostics.

### v0.8: Wi-Fi Transport

Goal:

- Add same protocol over LAN.

Deliverables:

- Manual IP connection.
- Basic pairing or connection confirmation.
- Transport abstraction validation.

### v0.9: Formal USB Re-evaluation

Goal:

- Revisit AOA after the product pipeline works.

Deliverables:

- AOA hello/pong retry.
- AOA throughput test.
- Decision on whether formal AOA transport enters v1.0.

---

## 7. Technical Risks

### 7.1 HDC Throughput Risk

HDC may not provide enough stable bandwidth for high-resolution 60 FPS video.

Mitigation:

- Measure throughput early.
- Start with 1080p/60 or native/30.
- Avoid overcommitting to native/60 in the first MVP.

### 7.2 HarmonyOS Decoder Risk

HarmonyOS hardware decoder may not handle native resolution 60 FPS smoothly.

Mitigation:

- Test local H.265 files first.
- Use native/30 as MVP success line.
- Keep 1080p/60 fallback.

### 7.3 Windows Encoder Complexity

Different machines require different hardware encoder paths.

Mitigation:

- Start with RTX 4060 + NVENC.
- Add Intel path later.
- Keep H.264 fallback for compatibility.

### 7.4 AOA Data Path Risk

AOA control works, but data path is blocked.

Mitigation:

- Pause AOA.
- Preserve it as future transport.
- Do not depend on AOA for MVP.

### 7.5 Virtual Display Dependency

The project depends on external virtual display tools.

Mitigation:

- Document Parsec VDisplay workflow.
- Allow main display capture for testing.
- Clearly state that virtual display creation is out of scope.

---

## 8. Success Criteria for Current Prototype

A prototype is considered successful when:

- A Windows 11 PC can capture an existing display.
- The selected display may be a Parsec VDisplay virtual screen.
- The captured screen can be encoded as H.265.
- The encoded stream can be sent over HDC.
- The HarmonyOS MatePad can receive, decode, and display it fullscreen.
- The screen is readable for documents and web pages.
- Mouse cursor is visible.
- Native resolution at 30 FPS is usable, or a lower fallback mode is stable.
- The system can run for at least 30 minutes without crashing.
- No touch, audio, or remote-control features are required.

---

## 9. Long-Term Product Direction

The long-term direction is:

- Keep Tablet2Screen as an independent app.
- Borrow architectural ideas from Sunshine/Moonlight if useful.
- Do not fork the entire Sunshine/Moonlight stack unless necessary.
- Prefer reusable modules:
  - capture abstraction
  - encoder abstraction
  - transport abstraction
  - decoder abstraction
  - rendering abstraction

Possible future transports:

- HDC developer transport.
- Wi-Fi LAN transport.
- AOA formal USB transport.
- Other USB bridge transport only if necessary.

Possible future features:

- Automatic display selection.
- Automatic virtual display guidance.
- Pairing code.
- Device whitelist.
- Encrypted Wi-Fi stream.
- Better diagnostics.
- Installer.
- Open-source release.
- License review.

Features still not planned for early versions:

- Touch input forwarding.
- Stylus forwarding.
- Audio.
- Multi-tablet mode.
- Public internet remote access.
- Cloud relay.
- Account system.
- Self-developed virtual display driver.

---

## 10. Immediate Next Actions

The next concrete tasks are:

1. Build HDC ping/pong.
2. Measure HDC throughput.
3. Build HarmonyOS local H.265 decoder probe.
4. Build HDC H.265 file stream probe.
5. Build Windows screen capture + encoder probe.
6. Connect everything into live HDC screen streaming.
7. Only then revisit AOA.

Current priority:

    HDC ping/pong > HDC throughput > H.265 decode > H.265 file stream > live screen streaming > AOA revisit

Do not continue blocking on AOA data transfer at this stage.