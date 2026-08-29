# Media capture

## Principle

Every public image and video comes from the real application driven by the deterministic fixture through the real API, persistence engine and interface. Mock-ups, design renders and placeholder boxes are prohibited, and the repository policy check fails when a referenced file is missing, too small to be a real capture, or not a decodable image.

## Pipeline

1. `pnpm --dir apps/web build` produces the interface bundle into the API crate's embedded asset directory.
2. `cargo build -p heikas-cli` embeds that bundle.
3. `cargo run -p xtask -- demo` seeds a disposable fixture repository and drives a complete run to a commit through the real command line application.
4. A second run is created and left paused at plan approval so the capture can approve it and record a live repair loop.
5. `heikas ui` starts and prints its address and bootstrap URL as JSON.
6. `pnpm --dir apps/web capture` drives the browser at 1440 by 900, writes the four screenshots, saves a frame sequence and records the session video as WebM.
7. `cargo run -p xtask -- media` encodes the animation from the frame sequence and derives the MP4, then validates every reference.

## Encoding

The animation is encoded in Rust from the captured frames using a neural quantiser to build one shared palette across the sequence, so the loop has no palette flicker. Frames are box-filtered down to seven hundred and twenty pixels wide and every second frame is kept.

The MP4 is derived from the recorded WebM with an H.264 encoder. That is the only step that uses an external media tool, it runs only when regenerating documentation, and the product itself never depends on it at runtime.

## Validation

The policy check reads the PNG header to confirm the screenshots are exactly 1440 by 900, reads the GIF header to confirm the animation decodes, and enforces a minimum plausible size for each asset so an empty placeholder cannot pass. It also fails when the public README stops referencing any required file.
