use anyhow::{Context, Result, ensure};
use dnr_package::WindowIcon;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

/// Decode at packaging time so the runtime only needs a small, bounded RGBA image.
pub fn load(path: &Path) -> Result<WindowIcon> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "window icon PNG exceeds 8 MiB"
    );
    let mut decoder = png::Decoder::new_with_limits(
        BufReader::new(std::io::Cursor::new(bytes)),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let header = decoder
        .read_header_info()
        .context("window icon must be a PNG")?;
    ensure!(
        (1..=4096).contains(&header.width) && (1..=4096).contains(&header.height),
        "window icon PNG dimensions must be 1..4096"
    );
    let mut reader = decoder.read_info()?;
    ensure!(
        reader.info().animation_control.is_none(),
        "animated window icons are unsupported"
    );
    let mut pixels = vec![0; reader.output_buffer_size().context("oversized PNG")?];
    let info = reader
        .next_frame(&mut pixels)
        .context("decoding window icon")?;
    reader.finish()?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => anyhow::bail!("unsupported PNG color type"),
    };
    let largest = info.width.max(info.height);
    let width = if largest > 128 {
        (info.width * 128 / largest).max(1)
    } else {
        info.width
    };
    let height = if largest > 128 {
        (info.height * 128 / largest).max(1)
    } else {
        info.height
    };
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    // Box average in premultiplied space avoids dark fringes around transparent icons.
    for y in 0..height {
        for x in 0..width {
            let mut sums = [0u64; 4];
            let mut count = 0;
            for sy in y * info.height / height..(y + 1) * info.height / height {
                for sx in x * info.width / width..(x + 1) * info.width / width {
                    let i = ((sy * info.width + sx) * channels) as usize;
                    let p = &pixels[i..i + channels as usize];
                    let [r, g, b, a] = match channels {
                        1 => [p[0], p[0], p[0], 255],
                        2 => [p[0], p[0], p[0], p[1]],
                        3 => [p[0], p[1], p[2], 255],
                        _ => [p[0], p[1], p[2], p[3]],
                    };
                    for (sum, c) in sums[..3].iter_mut().zip([r, g, b]) {
                        *sum += u64::from(c) * u64::from(a);
                    }
                    sums[3] += u64::from(a);
                    count += 1;
                }
            }
            for c in &sums[..3] {
                rgba.push(c.checked_div(sums[3]).unwrap_or(0) as u8);
            }
            rgba.push((sums[3] / count) as u8);
        }
    }
    Ok(WindowIcon {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_downsample_preserves_alpha_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 256, 128);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let pixels: Vec<_> = (0..256 * 128)
                .flat_map(|i| {
                    if i % 2 == 0 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 0]
                    }
                })
                .collect();
            writer.write_image_data(&pixels).unwrap();
        }
        std::fs::write(&path, &bytes).unwrap();
        let icon = load(&path).unwrap();
        assert_eq!((icon.width, icon.height), (128, 64));
        assert!(
            icon.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [255, 0, 0, 127])
        );
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
    }
}
