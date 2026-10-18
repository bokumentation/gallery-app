//! `.cube` LUTs: text parsing, persistent storage in the app cache and
//! conversion into an RGBA 3D sample texture for the grade shader. Parsing is
//! a plain streaming scan (a 33³ or 65³ file is small enough that no worker
//! thread is needed).
//!
//! Sampling happens in display space (sRGB-encoded), the domain most artistic
//! `.cube` files ship in.

const MAX_LUT_SIZE: u32 = 128;

#[derive(Clone, Debug)]
pub struct CubeLut {
    pub name: String,
    pub size: u32,
    /// `size^3` RGBA texels indexed `r + g·n + b·n²` (r fastest): the file
    /// order — red advances along a line, green per line, blue per page.
    pub data: Vec<u8>,
}

fn parse_entry(line: &str) -> Option<[f32; 3]> {
    let mut values = [0.0f32; 3];
    let mut count = 0;
    for token in line.split_ascii_whitespace() {
        match token.parse::<f32>() {
            Ok(value) if count < 3 => {
                values[count] = value;
                count += 1;
            }
            // Longer rows are LUT_1D remnants; reject the file.
            _ => return None,
        }
    }
    (count == 3).then_some(values)
}

fn encode(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl CubeLut {
    pub fn parse(name: String, text: &str) -> Option<CubeLut> {
        let mut size = None;
        let mut entry_index = 0usize;
        let mut data: Vec<u8> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
                continue;
            }
            if let Some(rest) = line.strip_prefix("LUT_3D_SIZE") {
                let parsed: u32 = rest.trim().parse().ok()?;
                if parsed == 0 || parsed > MAX_LUT_SIZE {
                    return None;
                }
                size = Some(parsed);
                continue;
            }
            if line.starts_with("LUT_1D_SIZE") || line.starts_with("DOMAIN_") {
                return None; // Only 3D LUTs are supported.
            }
            let Some(values) = parse_entry(line) else {
                continue;
            };
            let lut_size = size?; // Entries before the size header are invalid.
            let total = lut_size as usize * lut_size as usize * lut_size as usize;
            if entry_index >= total {
                return None;
            }
            data.resize((entry_index + 1) * 4, 0);
            for (channel, value) in values.into_iter().enumerate() {
                data[entry_index * 4 + channel] = encode(value);
            }
            data[entry_index * 4 + 3] = 255;
            entry_index += 1;
        }

        let size = size?;
        (data.len() == (size * size * size * 4) as usize)
            .then(|| CubeLut { name, size, data })
    }

    /// Persistent store: `~/.cache/rust-gallery/luts/*.cube`.
    pub fn store_dir() -> Option<std::path::PathBuf> {
        dirs::cache_dir().map(|dir| dir.join("rust-gallery").join("luts"))
    }

    pub fn list() -> Vec<String> {
        let Some(dir) = Self::store_dir() else {
            return Vec::new();
        };
        let mut names = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let lower = name.to_lowercase();
                lower.ends_with(".cube").then(|| name[..name.len() - 5].to_owned())
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub fn load(name: &str) -> Option<CubeLut> {
        let dir = Self::store_dir()?;
        let text = std::fs::read_to_string(dir.join(format!("{name}.cube"))).ok()?;
        CubeLut::parse(name.to_owned(), &text)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = Self::store_dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no cache directory")
        })?;
        std::fs::create_dir_all(&dir)?;
        self.write(dir.join(format!("{}.cube", self.name)))
    }

    /// Canonical `.cube` text in the same order the parser reads, so a store
    /// round trip reproduces identical texels.
    fn write(&self, path: std::path::PathBuf) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        writeln!(file, "# Gallery persisted LUT")?;
        writeln!(file, "LUT_3D_SIZE {}", self.size)?;
        let size = self.size;
        for blue in 0..size {
            for green in 0..size {
                for red in 0..size {
                    let index = (red + green * size + blue * size * size) as usize;
                    writeln!(
                        file,
                        "{:.6} {:.6} {:.6}",
                        self.data[index * 4] as f32 / 255.0,
                        self.data[index * 4 + 1] as f32 / 255.0,
                        self.data[index * 4 + 2] as f32 / 255.0,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Upload into an RGBA 3D texture. wgpu's 3D slice order is x fastest,
    /// then y, then z — the same `r + g·n + b·n²` layout the parser produces.
    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, label: &str) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: self.size,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.size * 4),
                rows_per_image: Some(self.size),
            },
            wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: self.size,
            },
        );
        texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lut() -> String {
        let mut text = String::from("TITLE Test\nLUT_3D_SIZE 2\n\n");
        let size = 2usize;
        for blue in 0..size {
            for green in 0..size {
                for red in 0..size {
                    text.push_str(&format!(
                        "{:.1} {:.1} {:.1}\n",
                        red as f32 * 0.4,
                        green as f32 * 0.4,
                        1.0 - blue as f32 * 0.5
                    ));
                }
            }
        }
        text
    }

    #[test]
    fn parses_linear_ramp() {
        let lut = CubeLut::parse("test".to_owned(), &sample_lut()).unwrap();
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data.len(), 8 * 4);
        // Texel (r=1, g=0, b=0) is index 1: r = 0.4 -> 102.
        assert_eq!(lut.data[1 * 4], 102);
        // Texel (0, 0, 0) is r=0, g=0, b=1.0.
        assert_eq!(lut.data[2], 255);
        assert_eq!(lut.data[3], 255);
    }

    #[test]
    fn rejects_sizeless_and_oversized() {
        assert!(CubeLut::parse("x".into(), "0 0 0\n1 1 1\n").is_none());
        assert!(CubeLut::parse("x".into(), "LUT_3D_SIZE 9999\n").is_none());
        assert!(CubeLut::parse("x".into(), "LUT_1D_SIZE 4\n0 0 0\n").is_none());
    }

    #[test]
    fn round_trips_through_the_store() {
        let lut = CubeLut::parse("gallery-lut-roundtrip".to_owned(), &sample_lut()).unwrap();
        lut.save().unwrap();
        let reloaded = CubeLut::load("gallery-lut-roundtrip").unwrap();
        assert_eq!(reloaded.size, lut.size);
        assert_eq!(reloaded.data, lut.data);
        let dir = CubeLut::store_dir().unwrap();
        let _ = std::fs::remove_file(dir.join("gallery-lut-roundtrip.cube"));
    }

    #[test]
    fn lists_persisted_names() {
        let lut = CubeLut::parse("gallery-lut-listed".to_owned(), &sample_lut()).unwrap();
        lut.save().unwrap();
        assert!(CubeLut::list().contains(&"gallery-lut-listed".to_owned()));
        let dir = CubeLut::store_dir().unwrap();
        let _ = std::fs::remove_file(dir.join("gallery-lut-listed.cube"));
    }
}
