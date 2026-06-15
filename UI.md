# Tablet2Screen UI Design Philosophy (Codex Implementation Spec)

## 1. Core Design Philosophy

Tablet2Screen is not a traditional multi-page application.

It is a **state-driven visual system** whose purpose is to:

- Make connection state immediately observable
- Make streaming state visually continuous
- Make failure diagnosable without reading logs
- Minimize cognitive load during operation

### Fundamental Principle

> UI is a reflection of system state, not a navigation structure.

There is no “feature browsing”.
There is only:
- connection
- streaming
- health
- recovery

Everything else is secondary.

---

## 2. Interaction Model

The entire system follows a strict state machine:

```
Idle → Searching → Pairing → Connecting → Connected → Streaming
                         ↘ Error → Recovery → (previous state)
```

### UI Rule

- UI must NEVER require manual page switching to reflect state changes
- UI must ALWAYS auto-transition when state changes
- UI must NEVER expose internal architecture (no “modules”, “panels”, “tabs” as primary navigation)

---

## 3. Visual Language

### 3.1 Background System

The background is not a solid color.

It is a **soft atmospheric field** composed of:

- multiple overlapping pastel gradients
- radial blur layers
- low-frequency noise texture (optional)

Allowed palette family:
- soft blue
- soft purple
- soft orange
- soft green
- soft pink

### Constraints

- no harsh contrast regions
- no sharp gradient transitions
- no dominant single color
- background must feel “ambient”, not “designed”

---

### 3.2 Card System

UI content is organized into layered glass-like cards.

#### Layer hierarchy:

1. Base Cards (primary information containers)
   - semi-transparent white
   - blurred background (glass effect)
   - rounded corners
   - slight shadow

2. Secondary Cards (nested information)
   - no fill
   - only border + subtle hover highlight
   - visually lighter than base cards

3. Interaction Elements (buttons / toggles)
   - higher opacity than cards
   - clearly clickable
   - strong state feedback (hover / active)

---

### 3.3 Visual Priority Rule

- State > Content > Decoration

Meaning:
- connection status must be most visible
- data is secondary
- decoration must never compete with state visibility

---

## 4. Desktop UI Philosophy

### 4.1 Structural Constraint

Desktop UI must NOT use sidebar navigation.

Instead:

- single dashboard surface
- grid-based card layout
- floating status emphasis

### Layout Model

```
[ Custom Title Bar ]
[ Global Status Strip ]
[ Card Grid Dashboard ]
```

---

### 4.2 Dashboard Composition

Dashboard consists of four conceptual blocks:

- Connection state
- Display state
- Streaming state
- System health

Each block:
- self-contained
- independently updated
- visually consistent

---

### 4.3 Title Bar Requirements

Desktop app must implement a fully custom title bar:

Required capabilities:
- window drag
- minimize
- maximize / restore toggle
- close
- double-click maximize/restore

Interaction rules:
- hover reveals controls
- active click provides tactile feedback animation
- disabled state must be visually distinct

---

### 4.4 Desktop Interaction Style

All interactions follow:

- smooth transitions (200–300ms)
- no instant state switching
- visual confirmation for every action
- subtle motion for feedback (scale / opacity / blur changes)

---

## 5. Tablet UI Philosophy

### 5.1 Dual-Mode Structure

Tablet app has exactly two modes:

#### Mode A: Connection Mode
- pairing screen
- device availability
- transport selection
- settings entry

#### Mode B: Display Mode
- fullscreen rendering surface
- no persistent UI

---

### 5.2 Mode Transition Rule

```
Connected → immediately enter Display Mode
Disconnected → immediately return to Connection Mode
```

No manual navigation is allowed between modes.

---

### 5.3 Display Mode Philosophy

Display mode is intentionally minimal:

- fullscreen video surface
- no permanent UI
- no navigation
- no system chrome

Only one persistent element exists:
- floating control button

---

### 5.4 Floating Control Button

Purpose:
- temporary access to system controls

Behavior:
- low opacity by default
- becomes prominent on hover/tap
- opens transient overlay panel

Must NOT interfere with content visibility.

---

### 5.5 Overlay Panel

Overlay is:
- temporary
- auto-dismiss (timeout-based)
- glass-style translucent surface

Contains:
- connection status
- stream stats
- reconnect / disconnect actions
- diagnostics entry

---

## 6. State Visualization Philosophy

Every meaningful system state must be visible in UI:

### Required observable states:

- connection status
- transport mode
- streaming status
- latency / FPS / bitrate (when relevant)
- error conditions (if any)

### Rule:

> If a state affects behavior, it must be visible.

But:
- do not overload UI with raw logs
- only surface aggregated meaningful metrics

---

## 7. Error Representation Philosophy

Errors are structured in three layers:

1. Human-readable summary
2. Suggested recovery actions
3. Technical diagnostic details (optional expansion)

### Rule:

- Never show “generic failure”
- Every error must explain:
  - what is broken
  - why it is broken (high level)
  - how to recover

---

## 8. Animation Philosophy

Animations are functional, not decorative.

Allowed usage:
- state transitions
- button feedback
- modal appearance/disappearance
- connection status changes

Forbidden:
- idle animations
- decorative motion loops
- distracting effects

---

## 9. Responsiveness Rules

UI must adapt to:

- different tablet aspect ratios
- desktop window resizing

Constraints:
- layout must degrade gracefully
- no layout breakage under resizing
- core state elements must remain visible

---

## 10. Implementation Guidance for Codex

Codex should treat this as a **design constraint system**, not a layout spec.

### It is free to decide:

- exact layout arrangement
- component structure
- visual hierarchy details
- animation curves
- spacing system

### It must strictly follow:

- state-driven UI architecture
- dual-mode tablet model
- no sidebar desktop constraint
- ambient gradient background system
- glass-card hierarchy system
- custom title bar requirement
- minimal tablet display mode

---

## 11. Final Principle

> The system should feel like a living connection channel, not a software dashboard.

Users should perceive:

- “it is connected”
- “it is streaming”
- “it is stable / unstable”

without needing to interpret UI structure.

---