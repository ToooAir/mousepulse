//! Battery tray icon rasterizer. Pixels are straight-alpha 0xAARRGGBB, row-major, top-down,
//! which is what a 32bpp DIB for CreateIconIndirect expects.

/// Lightning bolt in 16-unit icon coordinates, drawn over the battery when charging.
const BOLT: [(f32, f32); 6] = [
    (9.6, 1.0),
    (3.6, 9.2),
    (7.4, 9.2),
    (5.8, 15.0),
    (12.4, 6.4),
    (8.6, 6.4),
];

/// Render a horizontal battery `size`×`size` px.
/// `level` is the percent to fill (None = disconnected: outline only); when `charging`,
/// a bolt in the `fill` color replaces the bar.
pub fn battery(
    size: usize,
    level: Option<u8>,
    charging: bool,
    outline: u32,
    fill: u32,
) -> Vec<u32> {
    let k = size as f32 / 16.0;
    let px = |v: f32| ((v * k).round() as usize).min(size);
    let t = px(1.0).max(1); // stroke width
    let mut img = vec![0u32; size * size];

    let mut rect = |x0: usize, y0: usize, x1: usize, y1: usize, rgb: u32| {
        for y in y0..y1 {
            for x in x0..x1 {
                img[y * size + x] = 0xFF00_0000 | rgb;
            }
        }
    };

    // Body outline, then the terminal nub on the right.
    let (x0, y0, x1, y1) = (px(1.0), px(3.5), px(14.0), px(12.5));
    rect(x0, y0, x1, y0 + t, outline);
    rect(x0, y1 - t, x1, y1, outline);
    rect(x0, y0, x0 + t, y1, outline);
    rect(x1 - t, y0, x1, y1, outline);
    rect(x1, px(6.0), px(15.5), px(10.0), outline);

    // Fill bar inside a one-stroke transparent gap; any non-zero level shows at least 1px.
    // While charging the bolt carries the level color instead, which reads cleaner at 16px.
    if let (Some(level), false) = (level, charging) {
        let (fx0, fy0, fx1, fy1) = (x0 + 2 * t, y0 + 2 * t, x1 - 2 * t, y1 - 2 * t);
        let w = ((fx1 - fx0) as f32 * level.min(100) as f32 / 100.0).round() as usize;
        let w = if level > 0 { w.max(1) } else { 0 };
        rect(fx0, fy0, fx0 + w, fy1, fill);
    }

    if charging {
        let bolt: Vec<(f32, f32)> = BOLT.iter().map(|&(x, y)| (x * k, y * k)).collect();
        // Fraction of a 4x4 supersample grid inside the bolt, optionally shifted.
        let cover = |x: usize, y: usize, dx: f32, dy: f32| {
            (0..16)
                .filter(|i| {
                    let sx = x as f32 + (i % 4) as f32 / 4.0 + 0.125 + dx;
                    let sy = y as f32 + (i / 4) as f32 / 4.0 + 0.125 + dy;
                    inside(&bolt, sx, sy)
                })
                .count() as u32
                * 255
                / 16
        };
        // Knock out a transparent halo around the bolt so it separates from outline and fill.
        let r = k.max(1.0);
        let ring = [
            (-r, 0.0),
            (r, 0.0),
            (0.0, -r),
            (0.0, r),
            (-r, -r),
            (r, -r),
            (-r, r),
            (r, r),
        ];
        for y in 0..size {
            for x in 0..size {
                let halo = ring
                    .iter()
                    .map(|&(dx, dy)| cover(x, y, dx, dy))
                    .max()
                    .unwrap_or(0);
                let p = &mut img[y * size + x];
                let a = (*p >> 24) * (255 - halo) / 255;
                *p = (a << 24) | (*p & 0x00FF_FFFF);
                let a = cover(x, y, 0.0, 0.0);
                if a > 0 {
                    *p = over(*p, fill, a);
                }
            }
        }
    }
    img
}

/// Even-odd point-in-polygon test.
fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Straight-alpha "source over destination" for one pixel.
fn over(dst: u32, rgb: u32, a: u32) -> u32 {
    let da = dst >> 24;
    let out_a = a + da * (255 - a) / 255;
    if out_a == 0 {
        return 0;
    }
    let ch = |shift: u32| {
        let (s, d) = ((rgb >> shift) & 0xFF, (dst >> shift) & 0xFF);
        (s * a + d * da * (255 - a) / 255) / out_a
    };
    (out_a << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: u32 = 0xFFFFFF;
    const GREEN: u32 = 0x09BB3B;

    fn count(img: &[u32], rgb: u32) -> usize {
        img.iter().filter(|&&p| p == 0xFF00_0000 | rgb).count()
    }

    #[test]
    fn fill_tracks_level() {
        for size in [16, 20, 24, 32] {
            let half = count(&battery(size, Some(50), false, WHITE, GREEN), GREEN);
            let full = count(&battery(size, Some(100), false, WHITE, GREEN), GREEN);
            assert!(
                half > 0 && full > half,
                "size {size}: half {half}, full {full}"
            );
            assert!(count(&battery(size, Some(5), false, WHITE, GREEN), GREEN) > 0);
            assert_eq!(
                count(&battery(size, Some(0), false, WHITE, GREEN), GREEN),
                0
            );
            assert_eq!(count(&battery(size, None, false, WHITE, GREEN), GREEN), 0);
        }
    }

    #[test]
    fn charging_draws_bolt_instead_of_bar() {
        for size in [16, 24, 32] {
            let bar = count(&battery(size, Some(100), false, WHITE, GREEN), GREEN);
            let bolt = count(&battery(size, Some(100), true, WHITE, GREEN), GREEN);
            // Bolt is drawn even at 0%, and is not a full-width bar.
            assert!(
                bolt > 0 && bolt != bar,
                "size {size}: bar {bar}, bolt {bolt}"
            );
            assert!(count(&battery(size, Some(0), true, WHITE, GREEN), GREEN) > 0);
        }
    }
}
