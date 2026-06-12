# Tablet2Screen Probe Findings: Phases 1-5

Date: 2026-06-11

Target devices:

- Windows 11 host
- Huawei MatePad Air 2024 / HarmonyOS 6
- HDC executable: `D:\Program\Huawei\DevEco Studio\sdk\default\openharmony\toolchains\hdc.exe`

## Executive Summary

HDC is viable as the prototype USB transport. Basic TCP communication works, sustained throughput around 100 Mbps is practical, and the tablet can decode local native-resolution H.265 including native 60 FPS. Live screen projection through a media-player style H.265 stream works but has unacceptable latency for second-screen use because the HarmonyOS `Video` component buffers the stream.

The accepted Phase 5 direction is native H.265 packet streaming: Windows sends compressed H.265 Annex-B packets over HDC, and HarmonyOS receives them with a native C++/NAPI module using `OH_AVCodec` and XComponent surface rendering.

Probe source scripts and phase runbooks are archived under:

```text
Server/probes/
```

Generated logs, Python cache, and generated server-side MP4 sample outputs were removed during cleanup.

## Phase 1: HDC Ping/Pong

Status: Passed.

Implementation:

- HarmonyOS app listens on `127.0.0.1:7000`.
- HDC forward maps Windows `127.0.0.1:17000` to tablet `127.0.0.1:7000`.
- Windows sends `hello-from-windows`.
- Tablet replies `pong-from-harmony`.

Observed result:

```text
Received: b'pong-from-harmony'
SUCCESS: received pong-from-harmony
```

Finding:

- HDC socket-style communication is reliable enough to continue.

## Phase 2: HDC Throughput

Status: Passed for prototype needs.

Windows sender results:

| Test | Duration | Windows average |
| --- | ---: | ---: |
| 10 Mbps target | 10 s | 10.00 Mbps |
| 30 Mbps target | 10 s | 30.00 Mbps |
| 60 Mbps target | 10 s | 60.00 Mbps |
| 100 Mbps target | 10 s | 100.00 Mbps |
| 150 Mbps target | 10 s | 150.00 Mbps sender-side |
| Max sender rate | 10 s | about 125-157 Mbps across runs |
| 100 Mbps stability | 30 s | 100.00 Mbps |

Tablet-side observations:

- 60 Mbps and 100 Mbps passed.
- 150 Mbps was variable: initially around 130 Mbps, later up to around 180 Mbps on tablet-side counters.

Important correction:

- Early tablet receive rates were artificially low because the receiver converted every binary block to text. Removing that conversion significantly improved receiver-side results.

Finding:

- HDC can carry enough bandwidth for early H.265 screen streaming.
- 100 Mbps is a reasonable practical target.

## Phase 3: Local HarmonyOS H.265 Decoder

Status: Passed.

Implementation:

- Generated HEVC MP4 samples with FFmpeg.
- Bundled samples into app rawfile resources because direct HDC push to app sandbox failed with permission denied.
- Used HarmonyOS `Video` component for local hardware decode sanity testing.

Samples:

```text
hevc_1080p60.mp4
hevc_2800x1840_30.mp4
hevc_2800x1840_60.mp4
```

Observed result:

- All samples played well.
- Native `2800 x 1840 @ 60 FPS` is considered doable on the tablet decoder.

Finding:

- The tablet hardware decode capability is sufficient for the target class.

## Phase 4A: HDC HEVC File Stream Over HTTP

Status: Passed as a transport/decode probe, but not suitable as final architecture.

Implementation:

- Windows served HEVC MP4 files over HTTP byte ranges on `127.0.0.1:18002`.
- HDC reverse port mapped tablet `127.0.0.1:7002` to Windows `127.0.0.1:18002`.
- HarmonyOS `Video` component played streamed file URLs.

Important debugging result:

- Initially `Video` did not issue requests until the component was recreated with autoplay.
- Adding a `Test URL` button using HarmonyOS HTTP APIs proved HDC reverse-port networking worked independently of `Video`.

Observed result:

- Tablet sent `HEAD` and `GET`/`206` byte-range requests.
- Video playback worked.

Finding:

- HDC reverse port plus HTTP media stream works.
- This is useful for probing decoder/network behavior, but it is not the final second-screen model.

## Phase 4B: Live MJPEG Frame Probe

Status: Passed as live-frame architecture probe, failed for native-resolution performance.

Implementation:

- Windows generated synthetic frames with FFmpeg.
- Each frame was encoded as JPEG.
- Frames were sent over `tcp:17003 -> tcp:7003` with a small custom header.
- HarmonyOS decoded each JPEG to a `PixelMap` and rendered the newest frame, dropping stale pending frames.

Sender-side results:

| Test | Sender result |
| --- | ---: |
| 1280 x 800 @ 30 | about 30 FPS, 12.3 Mbps |
| 1920 x 1200 @ 30 | about 30 FPS, 21.4 Mbps |
| 1280 x 800 @ 60 | about 60 FPS, 24.8 Mbps |
| 1920 x 1080 @ 60 | about 60 FPS, 39.1 Mbps |
| 2800 x 1840 @ 30 | about 30 FPS, 54.8 Mbps |
| 2800 x 1840 @ 60 | about 60 FPS, 111 Mbps |

Tablet-side observations:

- Native `2800 x 1840 @ 60` rendered only about 13 FPS at JPEG quality 5.
- Lowering JPEG quality improved native 60 rendering to about 20 FPS, but still far from target.

Finding:

- HDC can carry live frame data at target rates.
- ArkTS JPEG decode/render is the bottleneck at native resolution.
- MJPEG is not viable for the final product.
- The useful design takeaway is the latest-frame policy: do not accumulate old frames; render newest data and drop stale data.

## Phase 5A: Current Screen H.265 Over HTTP/MPEG-TS

Status: Partially passed, not suitable for final low-latency second-screen use.

Implementation:

- Windows captured current desktop with FFmpeg `gdigrab` including mouse cursor.
- Encoded screen as H.265.
- Served MPEG-TS stream at `http://127.0.0.1:18004/live.ts`.
- HDC reverse port mapped tablet `127.0.0.1:7004` to Windows `127.0.0.1:18004`.
- HarmonyOS `Video` component played `http://127.0.0.1:7004/live.ts`.

Encoder findings:

- FFmpeg listed `hevc_nvenc`, but runtime failed with `Cannot load nvcuda.dll`.
- Runtime validation was added; server now falls back to `hevc_qsv` on this machine.
- `hevc_qsv` successfully produced a live H.265 stream.

Observed result:

- Tablet requested `/live.ts` and screen projection worked.
- Approximate delay was around 2 seconds.
- Low-latency server-side tuning made delay worse or did not solve the issue.

Finding:

- The delay is primarily from media-player buffering in HarmonyOS `Video`, not from HDC throughput.
- HTTP/MPEG-TS + `Video` is not the final path for an interactive second screen.

## Phase 5B: Native H.265 Packet Stream

Status: Passed as the selected Phase 5 solution.

Implementation:

- `phase5_h265_packet_sender.py` captures screen, encodes HEVC Annex B, parses NAL units, and sends custom `T2H5` packets over TCP.
- Intended HDC forward: Windows `127.0.0.1:17005` to tablet `127.0.0.1:7005`.
- HarmonyOS native module `libh265receiver.so` listens on `127.0.0.1:7005`.
- Native module parses `T2H5` packets, feeds H.265 NAL payloads into `OH_AVCodec`, and renders output to an XComponent surface.

HarmonyOS native files:

```text
entry/src/main/cpp/CMakeLists.txt
entry/src/main/cpp/napi_init.cpp
entry/src/main/cpp/h265_receiver.h
entry/src/main/cpp/h265_receiver.cpp
entry/src/main/cpp/types/libh265receiver/index.d.ts
entry/src/main/ets/native/H265Receiver.d.ts
```

UI:

- `Native H.265` tab
- XComponent surface
- Start/stop native receiver
- Stats: surface, decoder, packets, rendered outputs, dropped packets, bytes, last error

Packet header:

```text
magic        4 bytes  "T2H5"
sequence     uint32
timestamp_us uint64
flags        uint32
payload_len  uint32
payload      H.265 Annex-B NAL unit including start code
```

Finding:

- This is the correct transport and render direction for the real project.
- HDC packet transport, native packet parsing, `OH_AVCodec` H.265 decode, and XComponent rendering all work together.
- The current packet/drop logic is still a probe and should be refined before productization.

Successful run:

Windows sender command:

```powershell
python .\phase5_h265_packet_sender.py --fps 30 --bitrate 20M --scale 1920:1080 --duration 15
```

Windows result:

```text
nals=954
vcl=433
duration=15.143s
avg_mbps=19.02
```

Tablet native receiver result:

```text
Surface: true
Decoder: true
Error: 0
Packets: 954
Rendered: 342
Dropped: 77
Bytes: 36,007,403
```

## Final Technical Conclusions

- HDC transport is good enough for the prototype.
- The tablet can decode target H.265 resolutions locally.
- MJPEG proves live frame semantics but cannot meet native-resolution FPS due to JPEG decode cost.
- `Video` can decode H.265 but buffers too much for second-screen interaction.
- Native `OH_AVCodec` + surface/XComponent rendering is validated and should be used for the real MVP.

## Recommended Real Project Architecture

Windows host:

```text
Display capture
-> hardware H.265 encoder
-> H.265 Annex-B packetization
-> transport-independent packet protocol
-> HDC TCP transport first
```

HarmonyOS tablet:

```text
HDC TCP receiver
-> T2H5 / future protocol packet parser
-> native OH_AVCodec H.265 decoder
-> XComponent/native surface render
-> latest-frame/drop-stale policy
```

Protocol direction:

- Keep the `T2H5` packet idea, but evolve it toward the Roadmap Phase 7 binary framing format.
- Include stream config messages for codec, width, height, FPS, and VPS/SPS/PPS.
- Do not rely on HTTP, MPEG-TS, or `Video` for the final low-latency path.

## Recommended Next Milestone

Stabilize the native H.265 path:

```text
- group NALs into access units/frames instead of treating every NAL independently
- never drop VPS/SPS/PPS configuration packets
- drop only stale VCL frame data when behind
- add explicit stream config/control messages
- add keyframe request support
- tune GOP, bitrate, resolution, and FPS
- replace FFmpeg screen capture with Rust/Tauri capture and hardware encoder pipeline
```
