# Tablet2Screen UI Design

## 1. Design Principle

Tablet2Screen has two clients:

- **Desktop App**: control center.
- **Tablet App**: display receiver.

The UI should feel:

- Simple
- Modern
- Calm
- Technical but not intimidating
- Focused on connection status and display quality

Core product experience:

    Open tablet app.
    Open desktop app.
    Connect.
    Tablet becomes a second screen.

---

## 2. Visual Style

### 2.1 Overall Style

Use a clean modern utility style:

- Rounded cards
- Soft shadows
- Large status indicators
- Clear primary actions
- Minimal decorative elements
- Dense but readable layout on desktop
- Almost invisible UI during tablet display mode

The UI should feel closer to:

- Moonlight / Sunshine simplicity
- Windows 11 settings clarity
- Developer-tool level diagnostics when needed

Avoid:

- Gamer-style neon UI
- Overly colorful dashboards
- Too many icons
- Heavy gradients
- Complex animation

---

## 3. Color System

Use a restrained modern palette.

### 3.1 Light Theme

    Background:      #F7F8FA
    Card:            #FFFFFF
    Primary Text:    #111827
    Secondary Text:  #6B7280
    Border:          #E5E7EB
    Primary Blue:    #2563EB
    Success Green:   #16A34A
    Warning Amber:   #F59E0B
    Error Red:       #DC2626

### 3.2 Dark Theme

    Background:      #0F172A
    Card:            #111827
    Elevated Card:   #1F2937
    Primary Text:    #F9FAFB
    Secondary Text:  #9CA3AF
    Border:          #374151
    Primary Blue:    #3B82F6
    Success Green:   #22C55E
    Warning Amber:   #FBBF24
    Error Red:       #EF4444

### 3.3 Status Colors

    Connected:       Green
    Connecting:      Blue
    Degraded:        Amber
    Error:           Red
    Idle:            Gray

Keep color usage functional.
Do not color everything.

---

## 4. Typography

Use a modern sans-serif font.

Recommended desktop typography:

    Page title:       22-26px, semibold
    Section title:    16-18px, semibold
    Body text:        14px
    Secondary text:   13px
    Status text:      13-14px, medium
    Logs:             12-13px, monospace

Recommended tablet typography:

    Pairing code:     36-48px, semibold
    Status title:     22-28px
    Body text:        15-16px
    Overlay text:     13-14px

---

# 5. Desktop App UI

## 5.1 Desktop Layout

Use a left sidebar + card dashboard layout.

    ┌───────────────┬──────────────────────────────────────┐
    │ Tablet2Screen │ Dashboard                            │
    │               │                                      │
    │ ● Dashboard   │ ┌──────────────┐ ┌──────────────┐    │
    │   Devices     │ │ Connection   │ │ Display      │    │
    │   Display     │ └──────────────┘ └──────────────┘    │
    │   Stream      │ ┌──────────────┐ ┌──────────────┐    │
    │   Diagnostics │ │ Stream       │ │ Health       │    │
    │   Settings    │ └──────────────┘ └──────────────┘    │
    └───────────────┴──────────────────────────────────────┘

Style:

- Sidebar width: about 200px
- Content max width: 900-1100px
- Cards use 12-16px radius
- Use generous spacing
- Important buttons should be visually obvious

---

## 5.2 Desktop Dashboard

The dashboard should answer immediately:

- Is the tablet connected?
- Which transport is active?
- Is the virtual display active?
- Is the stream running?
- Is performance healthy?

Main cards:

    Connection
    - Device name
    - Connection state
    - Transport mode
    - Connect / Disconnect

    Display
    - Virtual display state
    - Resolution
    - Refresh rate
    - Open Windows display settings

    Stream
    - Quality preset
    - Codec
    - Bitrate
    - FPS
    - Start / Stop streaming

    Health
    - Current FPS
    - Latency
    - Dropped frames
    - Connection quality

Recommended dashboard copy:

    Connected to MatePad BKY-W20
    USB Direct
    Virtual display active
    Streaming 1920x1200 @ 60 FPS

---

## 5.3 Devices Page

Purpose:

- Show detected tablets.
- Let the user choose Wi-Fi or USB.
- Handle pairing.

Device card:

    MatePad BKY-W20
    Status: Available
    Wi-Fi: Available
    USB: Detected

    [Connect via USB] [Connect via Wi-Fi]

Transport selector:

    Auto
    USB Direct
    Wi-Fi LAN

Important wording:

    USB Direct uses the USB cable as a private data channel.
    It does not require USB tethering.

---

## 5.4 Display Page

Purpose:

- Manage the virtual display.
- Select display mode and resolution.

Options:

    Virtual Display
    - Active / Missing / Error

    Display Mode
    - Extend desktop
    - Mirror primary display

    Resolution
    - 1280x800
    - 1600x1000
    - 1920x1200
    - Native

    Refresh Rate
    - 30 Hz
    - 60 Hz

    Orientation
    - Landscape
    - Portrait

Primary action:

    [Create Virtual Display]

Secondary action:

    [Open Windows Display Settings]

For MVP, prioritize:

    Extend desktop
    1920x1200
    60 Hz
    Landscape

---

## 5.5 Stream Page

Purpose:

- Let users choose between clarity, smoothness, and power usage.

Quality presets:

    Smooth
    - Lower resolution
    - 60 FPS
    - Lower bitrate
    - More stable

    Balanced
    - 1920x1200
    - 60 FPS
    - Medium bitrate
    - Default choice

    Sharp
    - Higher resolution
    - Higher bitrate
    - Better text clarity

    Battery Saver
    - 30 FPS
    - Lower bitrate
    - Lower tablet heat

Advanced settings should be collapsed by default.

Advanced options:

    Encoder: Auto / Hardware H.264 / Software
    Bitrate: Auto / Manual
    Latency Mode: Low Latency / Balanced / Quality

---

## 5.6 Diagnostics Page

Diagnostics should be clear and layered.

Show checks like:

    ✓ Tablet detected
    ✓ Pairing accepted
    ✓ Virtual display active
    ✓ Encoder available
    ✓ Stream started
    ✕ USB data channel unavailable

Show session stats:

    Transport: USB Direct
    Resolution: 1920x1200
    FPS: 60
    Bitrate: 18 Mbps
    Latency: 23 ms
    Dropped frames: 0.3%

Log viewer:

    [12:01:23] Tablet detected: MatePad BKY-W20
    [12:01:24] USB transport opened
    [12:01:25] Virtual display active
    [12:01:26] Stream started

Actions:

    [Restart Transport]
    [Restart Stream]
    [Copy Logs]
    [Export Debug Report]

---

# 6. Tablet App UI

## 6.1 Tablet Design Principle

The tablet app should be almost invisible after connection.

Before connection:

- Show pairing and waiting state clearly.

After connection:

- Enter fullscreen display mode.
- Hide all controls.
- Show only a small floating control button.

---

## 6.2 Tablet Waiting Screen

Layout:

    ┌────────────────────────────────┐
    │                                │
    │        Tablet2Screen           │
    │  Use this tablet as a display  │
    │                                │
    │        Pairing Code            │
    │          482 913               │
    │                                │
    │  Wi-Fi LAN: Available          │
    │  USB Direct: Cable connected   │
    │                                │
    │  Waiting for desktop...        │
    │                                │
    │  [Settings] [Diagnostics]      │
    │                                │
    └────────────────────────────────┘

Style:

- Centered layout
- Large pairing code
- Minimal text
- Clear transport availability
- Calm dark background preferred

---

## 6.3 Tablet Fullscreen Display

Once connected:

    ┌────────────────────────────────┐
    │                                │
    │                                │
    │      Fullscreen Video Surface  │
    │                                │
    │                         ●      │
    │                   Floating Btn │
    │                                │
    └────────────────────────────────┘

Style:

- Fullscreen video
- No title bar
- No bottom navigation
- No permanent toolbar
- Floating button opacity around 40-60%
- Auto-hide overlay

---

## 6.4 Tablet Control Overlay

Triggered by tapping the floating button.

    ┌────────────────────────────┐
    │ Tablet2Screen              │
    │                            │
    │ Status: Connected          │
    │ Transport: USB Direct      │
    │ Resolution: 1920x1200      │
    │ FPS: 60                    │
    │ Latency: 23 ms             │
    │                            │
    │ Display Mode               │
    │ [Fit Screen] [Original]    │
    │                            │
    │ [Reconnect] [Disconnect]   │
    │ [Diagnostics] [Settings]   │
    └────────────────────────────┘

Overlay style:

- Frosted glass / translucent dark card
- Rounded corners
- Compact layout
- Auto-hide after 5 seconds
- Tap outside to close

---

## 6.5 Tablet Debug Overlay

Developer mode only.

    FPS 59.8 | 18 Mbps | 23 ms | USB

Display position:

    Top-left corner

Fields:

    FPS
    Bitrate
    Latency
    Dropped frames
    Transport
    Decoder type

Default:

    Hidden

---

# 7. Connection States

Use consistent states on both desktop and tablet.

    Idle
    Searching
    Device Found
    Pairing
    Connecting
    Connected
    Streaming
    Reconnecting
    Disconnected
    Error

Example desktop messages:

    Searching
    Looking for tablets over Wi-Fi and USB.

    Connected
    MatePad BKY-W20 is ready.

    Streaming
    Sending virtual display to tablet.

    Error
    Tablet detected, but USB data channel could not be opened.

Example tablet messages:

    Waiting
    Open the desktop app to connect.

    Connected
    Waiting for stream.

    Streaming
    Displaying secondary screen.

    Reconnecting
    Connection interrupted. Trying to reconnect.

---

# 8. Error UI

Use layered error messages.

Format:

    Title
    Human explanation
    Suggested actions
    Technical details

Example:

    USB Direct unavailable

    The tablet was detected, but the desktop app could not open the USB data channel.

    Try:
    - Reconnect the USB cable.
    - Restart the tablet app.
    - Switch to Wi-Fi LAN mode.

    Technical details:
    ACCESS_DENIED while opening USB interface.

Avoid vague errors like:

    Connection failed.

Prefer:

    Tablet detected, but transport failed.
    Virtual display active, but no frames are captured.
    Stream connected, but tablet decoder is not receiving frames.

---

# 9. MVP Screen Scope

## 9.1 Desktop MVP

Required screens:

    Dashboard
    Devices
    Display
    Stream
    Diagnostics
    Settings

Required controls:

    Connect / Disconnect
    Start / Stop streaming
    Select transport
    Select quality preset
    Select resolution
    Open Windows display settings
    Export logs

Postpone:

    Touch input
    Keyboard / mouse forwarding
    Audio streaming
    Multi-tablet support
    Cloud relay
    Full display arrangement editor

---

## 9.2 Tablet MVP

Required screens:

    Waiting / Pairing screen
    Fullscreen display screen
    Control overlay
    Diagnostics screen
    Settings screen

Required controls:

    Pairing code
    Reconnect
    Disconnect
    Fit screen
    Debug overlay toggle
    Keep screen awake

Postpone:

    Touch input
    Gesture control
    On-screen keyboard
    Audio
    QR pairing
    Custom themes

---

# 10. Recommended UI Stack

Desktop:

    Tauri
    React
    TypeScript
    Tailwind CSS
    shadcn/ui
    Zustand

Tablet:

    Native HarmonyOS / Android-style UI
    Native video rendering surface
    Minimal overlay components

Reason:

    Desktop needs a modern control panel.
    Tablet needs reliable fullscreen rendering more than complex UI.

---

# 11. Final Direction

The product should not look like a remote desktop app full of controls.

It should look like a clean display utility.

Best first version:

    Desktop:
    A professional control panel with clear status cards.

    Tablet:
    A quiet fullscreen receiver with only one floating control button.

Most important design sentence:

    Make connection state obvious, make streaming controls simple, and make failures diagnosable.