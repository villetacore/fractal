//! Экспорт кадра в PNG.
//!
//! Кодировщик написан вручную и не тянет зависимостей: PNG допускает
//! несжатые («stored») блоки deflate, чего для скриншотов достаточно.
//! Файлы получаются крупнее, зато формат честный и открывается везде.

use std::io;
use std::path::{Path, PathBuf};

use crate::camera::{Camera, View};
use crate::fractals::Fractal;
use crate::palette::Blend;
use crate::renderer::{Shading, color_of};
use crate::thread_pool::ThreadPool;

/// Посчитать фрактал в RGB-картинку с квадратными пикселями.
pub fn render_rgb(
    fractal: &dyn Fractal,
    view: View,
    palette: &Blend<'_>,
    shading: &Shading,
    size: (usize, usize),
    supersample: u32,
    pool: &ThreadPool,
) -> Vec<u8> {
    let width = size.0.max(1);
    let height = size.1.max(1);
    // char_aspect = 1: в картинке знакоместа квадратные.
    let camera = Camera::new(view, 1.0);
    let mapper = camera.mapper(width, height);
    let interior = palette.interior().scale(shading.fade);
    let ss = supersample.clamp(1, 4);

    let mut buf = vec![0u8; width * height * 3];
    pool.for_each_band(&mut buf, width * 3, |row, line| {
        for (col, px) in line.chunks_mut(3).enumerate() {
            let (mut r, mut g, mut b) = (0.0f64, 0.0f64, 0.0f64);
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = col as f64 + (sx as f64 + 0.5) / ss as f64;
                    let fy = row as f64 + (sy as f64 + 0.5) / ss as f64;
                    let (wx, wy) = mapper.world(fx, fy);
                    let c = color_of(fractal.sample(wx, wy), palette, shading, interior);
                    r += c.r as f64;
                    g += c.g as f64;
                    b += c.b as f64;
                }
            }
            let n = (ss * ss) as f64;
            px[0] = (r / n).round() as u8;
            px[1] = (g / n).round() as u8;
            px[2] = (b / n).round() as u8;
        }
    });
    buf
}

/// Сохранить RGB-буфер (3 байта на пиксель) в PNG.
pub fn save_png(path: &Path, rgb: &[u8], width: usize, height: usize) -> io::Result<()> {
    assert_eq!(rgb.len(), width * height * 3, "размер буфера не совпадает");
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, encode_png(rgb, width, height))
}

/// Собрать имя вида `frames/mandelbrot-000042.png`.
pub fn frame_path(dir: &Path, stem: &str, index: u64) -> PathBuf {
    dir.join(format!("{stem}-{index:06}.png"))
}

pub fn encode_png(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgb.len() + rgb.len() / 32 + 1024);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 бит, truecolor RGB, без фильтров и чересстрочности
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Сырые данные: перед каждой строкой байт типа фильтра (0 — без фильтра).
    let mut raw = Vec::with_capacity(height * (1 + width * 3));
    for y in 0..height {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * width * 3..(y + 1) * width * 3]);
    }

    write_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut out, b"IEND", &[]);
    out
}

/// zlib-поток из несжатых deflate-блоков.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.push(0x78); // CM = 8 (deflate), CINFO = 7 (окно 32K)
    out.push(0x01); // без словаря, минимальный уровень; (0x7801) % 31 == 0

    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        for (i, chunk) in data.chunks(65535).enumerate() {
            let last = (i + 1) * 65535 >= data.len();
            out.push(if last { 1 } else { 0 });
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32(0xFFFF_FFFF, kind);
    crc = crc32(crc, data);
    out.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
}

fn crc32(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    // 5552 — максимум байт до переполнения u32 при отложенном взятии остатка.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fractals::Mandelbrot;
    use crate::palette::builtin;

    #[test]
    fn crc32_matches_known_value() {
        // Эталон из спецификации zlib: CRC32("123456789") = 0xCBF43926.
        let crc = crc32(0xFFFF_FFFF, b"123456789") ^ 0xFFFF_FFFF;
        assert_eq!(crc, 0xCBF4_3926);
    }

    #[test]
    fn adler32_matches_known_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn zlib_header_checksum_is_valid() {
        let z = zlib_stored(b"hello");
        assert_eq!((z[0] as u16 * 256 + z[1] as u16) % 31, 0);
    }

    #[test]
    fn png_structure_is_well_formed() {
        let (w, h) = (4, 3);
        let rgb = vec![0x40u8; w * h * 3];
        let png = encode_png(&rgb, w, h);

        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &(w as u32).to_be_bytes());
        assert_eq!(&png[20..24], &(h as u32).to_be_bytes());
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");

        // Все длины и контрольные суммы чанков должны сходиться.
        let mut pos = 8;
        let mut seen = Vec::new();
        while pos < png.len() {
            let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
            let kind = &png[pos + 4..pos + 8];
            let data = &png[pos + 8..pos + 8 + len];
            let stored = u32::from_be_bytes(png[pos + 8 + len..pos + 12 + len].try_into().unwrap());
            let expect = (crc32(crc32(0xFFFF_FFFF, kind), data)) ^ 0xFFFF_FFFF;
            assert_eq!(stored, expect, "битая CRC у чанка {:?}", kind);
            seen.push(String::from_utf8_lossy(kind).to_string());
            pos += 12 + len;
        }
        assert_eq!(seen, vec!["IHDR", "IDAT", "IEND"]);
    }

    #[test]
    fn large_image_splits_into_several_deflate_blocks() {
        // Больше 65535 байт сырых данных — проверяем разбиение на блоки.
        let (w, h) = (200, 200);
        let rgb = vec![7u8; w * h * 3];
        let png = encode_png(&rgb, w, h);
        assert!(png.len() > w * h * 3);
    }

    #[test]
    fn renders_image_of_expected_size() {
        let palettes = builtin();
        let blend = Blend::single(&palettes[0]);
        let pool = ThreadPool::new(2);
        let rgb = render_rgb(
            &Mandelbrot::new(),
            View::new(-0.6, 0.0, 1.8),
            &blend,
            &Shading::default(),
            (32, 24),
            1,
            &pool,
        );
        assert_eq!(rgb.len(), 32 * 24 * 3);
        assert!(rgb.iter().any(|&b| b != rgb[0]), "картинка одноцветная");
    }

    #[test]
    fn frame_paths_are_zero_padded() {
        let p = frame_path(Path::new("frames"), "julia", 42);
        assert!(p.ends_with("julia-000042.png"));
    }
}
