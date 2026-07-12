//! COLORREF packing (0x00BBGGRR) matching Razer Chroma / Windows GDI.

/// Pack 0–255 RGB into a Chroma COLORREF.
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(b) << 16) | (u32::from(g) << 8) | u32::from(r)
}

/// Unpack COLORREF into (r, g, b).
pub fn unpack_rgb(color: u32) -> (u8, u8, u8) {
    let r = (color & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = ((color >> 16) & 0xFF) as u8;
    (r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        assert_eq!(unpack_rgb(rgb(255, 128, 0)), (255, 128, 0));
        assert_eq!(unpack_rgb(rgb(0, 0, 0)), (0, 0, 0));
        assert_eq!(unpack_rgb(rgb(255, 255, 255)), (255, 255, 255));
    }
}

