pub fn fit_into(
    src: &[u32],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
) -> Vec<u32> {
    let dst_width = dst_width.max(1);
    let dst_height = dst_height.max(1);
    let mut out = vec![0u32; dst_width.saturating_mul(dst_height)];
    if src_width == 0 || src_height == 0 || src.is_empty() {
        return out;
    }

    let (fit_w, fit_h) = fit_dimensions(src_width, src_height, dst_width, dst_height);
    let x0 = (dst_width - fit_w) / 2;
    let y0 = (dst_height - fit_h) / 2;

    for dy in 0..fit_h {
        let sy = dy.saturating_mul(src_height) / fit_h;
        let src_row = sy.saturating_mul(src_width);
        let dst_row = (y0 + dy).saturating_mul(dst_width) + x0;
        for dx in 0..fit_w {
            let sx = dx.saturating_mul(src_width) / fit_w;
            if let (Some(&pixel), Some(slot)) = (
                src.get(src_row + sx),
                out.get_mut(dst_row + dx),
            ) {
                *slot = pixel;
            }
        }
    }
    out
}

fn fit_dimensions(sw: usize, sh: usize, dw: usize, dh: usize) -> (usize, usize) {
    let lhs = (dw as u128).saturating_mul(sh as u128);
    let rhs = (dh as u128).saturating_mul(sw as u128);

    if lhs <= rhs {
        let h = ((sh as u128).saturating_mul(dw as u128) / sw as u128) as usize;
        (dw.max(1), h.max(1).min(dh))
    } else {
        let w = ((sw as u128).saturating_mul(dh as u128) / sh as u128) as usize;
        (w.max(1).min(dw), dh.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::fit_dimensions;

    #[test]
    fn keeps_aspect_ratio_inside_box() {
        assert_eq!(fit_dimensions(1920, 1080, 1000, 1000), (1000, 562));
        assert_eq!(fit_dimensions(1080, 1920, 1000, 1000), (562, 1000));
    }
}
