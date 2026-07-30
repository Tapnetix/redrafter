// The menu-bar icon's in-flight animation (SC11's visible half).
//
// A refine already flipped the tray's *tooltip* to "Refining…", which nobody
// sees unless they happen to hover the menu bar, and the frontend's spinner
// renders inside the settings window — which is hidden for the entire refine,
// since the whole point is that you triggered it from another app. So a slow
// refine looked like nothing was happening at all.
//
// Frames are drawn here as raw RGBA rather than shipped as PNG assets: it
// keeps the bundle unchanged, makes the geometry unit-testable on any host
// (this module is plain arithmetic, no Tauri), and lets the spinner be sized
// to whatever the tray asks for. Pixels are pure black with a varying alpha,
// which is what macOS template rendering wants — it recolours by alpha for
// light/dark menu bars and ignores RGB.

/// Number of dots in the ring, and therefore the number of distinct frames
/// before the animation repeats.
pub const SPINNER_FRAMES: usize = 12;

/// Side length (px) of a generated frame. 44 is the @2x size of the 22pt
/// macOS menu bar, so the ring lands on whole pixels at both scale factors.
pub const SPINNER_SIZE: u32 = 44;

/// Ring radius as a fraction of the frame size.
const RING_RADIUS: f32 = 0.34;
/// Dot radius as a fraction of the frame size.
const DOT_RADIUS: f32 = 0.085;
/// Alpha of the dimmest dot, as a fraction of the brightest.
const MIN_ALPHA: f32 = 0.12;

/// The alpha ramp for the dot `dot` on frame `frame`: the dot at the head of
/// the animation is fully opaque and the ones trailing behind it fade, which
/// is what reads as rotation.
fn dot_alpha(dot: usize, frame: usize) -> f32 {
    // How many steps this dot sits *behind* the head, counting backwards
    // around the ring. Getting this the other way round makes the bright
    // trail lead the head instead of following it, which reads as spinning
    // the wrong way.
    let behind = (frame % SPINNER_FRAMES + SPINNER_FRAMES - dot % SPINNER_FRAMES) % SPINNER_FRAMES;
    let t = behind as f32 / SPINNER_FRAMES as f32;
    // Linear fade from the head (t == 0) to the tail, floored at MIN_ALPHA so
    // the ring stays legible rather than vanishing to a single dot.
    MIN_ALPHA + (1.0 - MIN_ALPHA) * (1.0 - t)
}

/// Renders one frame of the spinner as RGBA8 (`size * size * 4` bytes).
///
/// Coverage is computed per pixel from the distance to each dot centre, giving
/// cheap anti-aliasing without pulling in a rasterizer.
pub fn spinner_frame_rgba(frame: usize, size: u32) -> Vec<u8> {
    let n = size as usize;
    let mut buf = vec![0u8; n * n * 4];
    let fsize = size as f32;
    let centre = fsize / 2.0;
    let ring = fsize * RING_RADIUS;
    let dot = fsize * DOT_RADIUS;

    // Pre-compute each dot's centre and alpha once rather than per pixel.
    let dots: Vec<(f32, f32, f32)> = (0..SPINNER_FRAMES)
        .map(|i| {
            // Start at 12 o'clock and go clockwise, matching every other
            // spinner the user sees on the platform.
            let angle = (i as f32 / SPINNER_FRAMES as f32) * std::f32::consts::TAU
                - std::f32::consts::FRAC_PI_2;
            (
                centre + ring * angle.cos(),
                centre + ring * angle.sin(),
                dot_alpha(i, frame),
            )
        })
        .collect();

    for y in 0..n {
        for x in 0..n {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut alpha: f32 = 0.0;
            for (cx, cy, a) in &dots {
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                // 1px feathered edge: full inside, zero outside, linear across.
                let coverage = ((dot - d) + 0.5).clamp(0.0, 1.0);
                alpha = alpha.max(coverage * a);
            }
            let i = (y * n + x) * 4;
            // Black; template rendering takes the shape from alpha alone.
            buf[i] = 0;
            buf[i + 1] = 0;
            buf[i + 2] = 0;
            buf[i + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(buf: &[u8], size: u32, x: u32, y: u32) -> u8 {
        buf[((y * size + x) * 4 + 3) as usize]
    }

    fn total_alpha(buf: &[u8]) -> u64 {
        buf.chunks_exact(4).map(|p| p[3] as u64).sum()
    }

    #[test]
    fn a_frame_is_rgba_of_the_requested_size() {
        let buf = spinner_frame_rgba(0, SPINNER_SIZE);
        assert_eq!(buf.len(), (SPINNER_SIZE * SPINNER_SIZE * 4) as usize);
    }

    #[test]
    fn every_pixel_is_black_so_macos_template_rendering_recolours_it() {
        let buf = spinner_frame_rgba(3, SPINNER_SIZE);
        for px in buf.chunks_exact(4) {
            assert_eq!((px[0], px[1], px[2]), (0, 0, 0));
        }
    }

    #[test]
    fn the_ring_is_hollow_and_does_not_touch_the_edges() {
        let buf = spinner_frame_rgba(0, SPINNER_SIZE);
        let mid = SPINNER_SIZE / 2;
        assert_eq!(alpha_at(&buf, SPINNER_SIZE, mid, mid), 0, "centre must be clear");
        for i in 0..SPINNER_SIZE {
            assert_eq!(alpha_at(&buf, SPINNER_SIZE, i, 0), 0, "top edge must be clear");
            assert_eq!(alpha_at(&buf, SPINNER_SIZE, 0, i), 0, "left edge must be clear");
        }
    }

    #[test]
    fn frames_differ_so_the_icon_visibly_rotates() {
        let a = spinner_frame_rgba(0, SPINNER_SIZE);
        let b = spinner_frame_rgba(1, SPINNER_SIZE);
        assert_ne!(a, b);
    }

    #[test]
    fn the_animation_loops_after_a_full_turn() {
        assert_eq!(
            spinner_frame_rgba(0, SPINNER_SIZE),
            spinner_frame_rgba(SPINNER_FRAMES, SPINNER_SIZE)
        );
    }

    #[test]
    fn total_ink_is_constant_across_frames_so_it_does_not_pulse() {
        // Rotation, not blinking: each frame carries the same set of alphas,
        // only assigned to different dots.
        let base = total_alpha(&spinner_frame_rgba(0, SPINNER_SIZE));
        for f in 1..SPINNER_FRAMES {
            let t = total_alpha(&spinner_frame_rgba(f, SPINNER_SIZE));
            let drift = (t as i64 - base as i64).abs();
            assert!(drift * 200 < base as i64, "frame {f} drifted by {drift} from {base}");
        }
    }

    #[test]
    fn the_head_dot_is_the_brightest_and_leads_the_rotation() {
        // Frame 0's head sits at 12 o'clock; frame 3's a quarter turn on.
        let size = SPINNER_SIZE;
        let top = spinner_frame_rgba(0, size);
        let quarter = spinner_frame_rgba(3, size);
        let mid = size / 2;
        let ring_px = (size as f32 * RING_RADIUS).round() as u32;

        let top_of_frame0 = alpha_at(&top, size, mid, mid - ring_px);
        let right_of_frame3 = alpha_at(&quarter, size, mid + ring_px, mid);
        assert!(top_of_frame0 > 200, "frame 0 head should be opaque, got {top_of_frame0}");
        assert!(
            right_of_frame3 > 200,
            "frame 3 head should have moved a quarter turn, got {right_of_frame3}"
        );
    }

    #[test]
    fn dot_alpha_fades_from_the_head_backwards() {
        assert!(dot_alpha(0, 0) > dot_alpha(11, 0));
        assert!(dot_alpha(11, 0) > dot_alpha(6, 0));
        assert!(dot_alpha(6, 0) >= MIN_ALPHA);
    }

    #[test]
    fn renders_at_an_arbitrary_size_without_panicking() {
        for size in [16u32, 22, 32, 44, 64] {
            let buf = spinner_frame_rgba(5, size);
            assert_eq!(buf.len(), (size * size * 4) as usize);
            assert!(total_alpha(&buf) > 0, "size {size} produced an empty frame");
        }
    }
}
