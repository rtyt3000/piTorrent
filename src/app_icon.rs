//! The piTorrent logo (rounded accent square with a download arrow), rasterized at runtime.

pub const SIZE: u32 = 64;

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0);
    let (cx, cy) = (a.0 + t * abx, a.1 + t * aby);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

pub fn rgba() -> Vec<u8> {
    let n = SIZE as f32;
    let (scale, offset) = (n / 24.0 * 0.62, n * 0.19);
    let at = |x: f32, y: f32| (offset + x * scale, offset + y * scale);
    let arrow = [
        (at(12.0, 17.0), at(12.0, 3.0)),
        (at(6.0, 11.0), at(12.0, 17.0)),
        (at(12.0, 17.0), at(18.0, 11.0)),
        (at(19.0, 21.0), at(5.0, 21.0)),
    ];
    let half_stroke = 2.4 * scale / 2.0;
    let radius = n * 0.26;
    let (bg, fg) = ([0x38u8, 0x71, 0xE1], [255u8, 255, 255]);

    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = ((p.0 - n / 2.0).abs() - (n / 2.0 - radius), (p.1 - n / 2.0).abs() - (n / 2.0 - radius));
            let sd = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
            let cover = (0.5 - sd).clamp(0.0, 1.0);
            let d = arrow.iter().map(|&(a, b)| dist_to_segment(p, a, b)).fold(f32::MAX, f32::min);
            let ink = (half_stroke + 0.5 - d).clamp(0.0, 1.0);
            for c in 0..3 {
                out.push((bg[c] as f32 * (1.0 - ink) + fg[c] as f32 * ink) as u8);
            }
            out.push((cover * 255.0) as u8);
        }
    }
    out
}
