use dng::ifd::{Ifd, IfdValue};
use dng::tags::{IfdType, MaybeKnownIfdFieldDescriptor};
use dng::{DngWriter, FileType};
use flate2::write::ZlibEncoder;
use flate2::Compression as FlateCompression;
use rawler::decoders::{RawDecodeParams, RawLoader as RawlerLoader};
use rawler::rawsource::RawSource;
use rawloader::{Orientation as RawOrientation, RawImageData, RawLoader};
use std::io::{Cursor, Write};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn convert_raw_to_dng(input: &[u8], _format: &str) -> Result<Vec<u8>, JsValue> {
    console_error_panic_hook::set_once();

    let loader = RawLoader::new();
    let raw = loader
        .decode(&mut Cursor::new(input), false)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse RAW: {:?}", e)))?;

    // Extract EXIF using rawler
    let source = RawSource::new_from_slice(input);
    let rawler_loader = RawlerLoader::new();
    let decoder = rawler_loader
        .get_decoder(&source)
        .map_err(|e| JsValue::from_str(&format!("Rawler failed to get decoder: {:?}", e)))?;
    let metadata = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .map_err(|e| JsValue::from_str(&format!("Rawler failed to get metadata: {:?}", e)))?;
    let exif = metadata.exif;

    // Orientation mapping
    let orientation = match raw.orientation {
        RawOrientation::Normal => 1u16,
        RawOrientation::Rotate90 => 6u16,
        RawOrientation::Rotate180 => 3u16,
        RawOrientation::Rotate270 => 8u16,
        _ => 1u16,
    };

    // --- Color Calibration (Sony ILCE-7M2 values) ---
    let color_matrix_vals = vec![5271, -712, -347, -6153, 13653, 2763, -1601, 2366, 7242];
    let color_matrix: Vec<IfdValue> = color_matrix_vals
        .into_iter()
        .map(|v| IfdValue::SRational(v, 10000))
        .collect();
    let matrix_list = IfdValue::List(color_matrix);

    // --- White Balance ---
    let mut neutral = Vec::new();
    if raw.wb_coeffs[0] > 0.0 && raw.wb_coeffs[1] > 0.0 && raw.wb_coeffs[2] > 0.0 {
        let r = raw.wb_coeffs[0];
        let g = raw.wb_coeffs[1];
        let b = raw.wb_coeffs[2];
        neutral = vec![
            IfdValue::Rational(1000000, (r / g * 1000000.0) as u32),
            IfdValue::Rational(1, 1),
            IfdValue::Rational(1000000, (b / g * 1000000.0) as u32),
        ];
    }

    // --- Thumbnail in IFD0 (1600px for high quality) ---
    let (thumb_data, thumb_w, thumb_h) = if let RawImageData::Integer(data) = &raw.data {
        let tw = 1600;
        let th = (raw.height * tw / raw.width) & !1;
        let mut tdata = Vec::with_capacity(tw * th * 3);
        let scale_x = raw.width as f32 / tw as f32;
        let scale_y = raw.height as f32 / th as f32;

        let black = raw.blacklevels[0] as f32;
        let white = raw.whitelevels[0] as f32;
        let range = (white - black).max(1.0f32);

        let r_coeff = raw.wb_coeffs[0] / raw.wb_coeffs[1];
        let b_coeff = raw.wb_coeffs[2] / raw.wb_coeffs[1];

        for y in 0..th {
            let y_start = (y as f32 * scale_y) as usize;
            let y_end = ((y + 1) as f32 * scale_y) as usize;
            for x in 0..tw {
                let x_start = (x as f32 * scale_x) as usize;
                let x_end = ((x + 1) as f32 * scale_x) as usize;

                let mut r_sum = 0.0f32;
                let mut g_sum = 0.0f32;
                let mut b_sum = 0.0f32;
                let (mut r_cnt, mut g_cnt, mut b_cnt) = (0, 0, 0);

                for sy in y_start..y_end.min(raw.height) {
                    let row_offset = sy * raw.width;
                    let is_even_row = sy % 2 == 0;
                    for sx in x_start..x_end.min(raw.width) {
                        let val = data[row_offset + sx] as f32;
                        if is_even_row {
                            if sx % 2 == 0 {
                                r_sum += val;
                                r_cnt += 1;
                            } else {
                                g_sum += val;
                                g_cnt += 1;
                            }
                        } else {
                            if sx % 2 == 0 {
                                g_sum += val;
                                g_cnt += 1;
                            } else {
                                b_sum += val;
                                b_cnt += 1;
                            }
                        }
                    }
                }

                let r_avg = if r_cnt > 0 {
                    (r_sum / r_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                };
                let g_avg = if g_cnt > 0 {
                    (g_sum / g_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                };
                let b_avg = if b_cnt > 0 {
                    (b_sum / b_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                };

                let gain = 5.0;
                let r8 = (r_avg * r_coeff * gain).powf(1.0 / 2.2).min(1.0) * 255.0;
                let g8 = (g_avg * 1.0 * gain).powf(1.0 / 2.2).min(1.0) * 255.0;
                let b8 = (b_avg * b_coeff * gain).powf(1.0 / 2.2).min(1.0) * 255.0;

                tdata.push(r8 as u8);
                tdata.push(g8 as u8);
                tdata.push(b8 as u8);
            }
        }
        (tdata, tw, th)
    } else {
        (vec![128u8; 16 * 16 * 3], 16, 16)
    };

    // --- SubIFD: RAW data ---
    let byte_data = match &raw.data {
        RawImageData::Integer(data) => {
            let mut bytes = Vec::with_capacity(data.len() * 2);
            for &x in data {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
            bytes
        }
        _ => return Err(JsValue::from_str("Unsupported data")),
    };

    // Compression (Adobe Deflate)
    let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
    encoder
        .write_all(&byte_data)
        .map_err(|e| JsValue::from_str(&format!("Failed to compress: {:?}", e)))?;
    let compressed_data = encoder
        .finish()
        .map_err(|e| JsValue::from_str(&format!("Failed to finish compress: {:?}", e)))?;
    let data_len = compressed_data.len() as u32;

    let black_level = raw.blacklevels[0] as u32;
    let white_level = raw.whitelevels[0] as u32;

    let mut sub_ifd = Ifd::new(IfdType::Ifd);
    // Insert in ascending order of Tag ID to satisfy TIFF spec
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x00fe, IfdType::Ifd),
        0u32,
    ); // NewSubfileType
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0100, IfdType::Ifd),
        raw.width as u32,
    ); // ImageWidth
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0101, IfdType::Ifd),
        raw.height as u32,
    ); // ImageLength
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0102, IfdType::Ifd),
        16u16,
    ); // BitsPerSample
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0103, IfdType::Ifd),
        8u16,
    ); // Compression (Adobe Deflate)
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0106, IfdType::Ifd),
        32803u16,
    ); // PhotometricInterpretation: CFA
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0111, IfdType::Ifd),
        IfdValue::Offsets(Arc::new(compressed_data)),
    ); // StripOffsets
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0115, IfdType::Ifd),
        1u16,
    ); // SamplesPerPixel
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0116, IfdType::Ifd),
        raw.height as u32,
    ); // RowsPerStrip
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0117, IfdType::Ifd),
        data_len,
    ); // StripByteCounts
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x011c, IfdType::Ifd),
        1u16,
    ); // PlanarConfiguration
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0153, IfdType::Ifd),
        1u16,
    ); // SampleFormat: Unsigned
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x828d, IfdType::Ifd),
        &[2u16, 2u16] as &[u16],
    ); // CFARepeatPatternDim
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x828e, IfdType::Ifd),
        &[0u8, 1, 1, 2] as &[u8],
    ); // CFAPattern: RGGB
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc616),
        &[0u8, 1, 2] as &[u8],
    ); // CFAPaneColor
    sub_ifd.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xc617), 1u16); // CFALayout
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc619, IfdType::Ifd),
        &[2u16, 2u16] as &[u16],
    ); // BlackLevelRepeatDim
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc61a, IfdType::Ifd),
        IfdValue::List(vec![IfdValue::Rational(black_level, 1); 4]),
    ); // BlackLevel
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc61d, IfdType::Ifd),
        white_level,
    ); // WhiteLevel
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc61e, IfdType::Ifd),
        IfdValue::List(vec![IfdValue::Rational(1, 1), IfdValue::Rational(1, 1)]),
    ); // DefaultScale
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc61f, IfdType::Ifd),
        IfdValue::List(vec![IfdValue::Short(0), IfdValue::Short(0)]),
    ); // DefaultCropOrigin
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc620, IfdType::Ifd),
        IfdValue::List(vec![
            IfdValue::Short(raw.width as u16),
            IfdValue::Short(raw.height as u16),
        ]),
    ); // DefaultCropSize
    sub_ifd.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xc62d), 0u16); // BayerGreenSplit
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc632),
        IfdValue::Rational(1, 1),
    ); // AntiAliasStrength
    sub_ifd.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xc65c), 1u16); // BestQualityScale
    sub_ifd.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc68d),
        &[0u16, 0, raw.height as u16, raw.width as u16] as &[u16],
    ); // ActiveArea

    // --- EXIF IFD ---
    let mut exif_ifd = Ifd::new(IfdType::Exif);
    if let Some(et) = exif.exposure_time {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x829a, IfdType::Exif),
            IfdValue::Rational(et.n, et.d),
        ); // ExposureTime
    }
    if let Some(ap) = exif.fnumber {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x829d, IfdType::Exif),
            IfdValue::Rational(ap.n, ap.d),
        ); // FNumber
    }
    if let Some(iso) = exif.iso_speed_ratings {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x8827, IfdType::Exif),
            iso,
        ); // ISOSpeedRatings
    }
    if let Some(fl) = exif.focal_length {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x920a, IfdType::Exif),
            IfdValue::Rational(fl.n, fl.d),
        ); // FocalLength
    }
    if let Some(dt) = &exif.date_time_original {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x9003, IfdType::Exif),
            dt.clone(),
        ); // DateTimeOriginal
    }
    if let Some(dt) = &exif.create_date {
        exif_ifd.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0x9004, IfdType::Exif),
            dt.clone(),
        ); // CreateDate
    }

    // --- IFD0: Metadata and Thumbnail ---
    let mut ifd0 = Ifd::new(IfdType::Ifd);
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x8769, IfdType::Ifd),
        IfdValue::Ifd(exif_ifd),
    ); // ExifIFDPointer
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x00fe, IfdType::Ifd),
        1u32,
    ); // NewSubfileType: Thumbnail
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0100, IfdType::Ifd),
        thumb_w as u32,
    ); // ImageWidth
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0101, IfdType::Ifd),
        thumb_h as u32,
    ); // ImageLength
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0102, IfdType::Ifd),
        &[8u16, 8, 8] as &[u16],
    ); // BitsPerSample
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0103, IfdType::Ifd),
        1u16,
    ); // Compression: None
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0106, IfdType::Ifd),
        2u16,
    ); // PhotometricInterpretation: RGB
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x010f, IfdType::Ifd),
        raw.make.clone(),
    ); // Make
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0110, IfdType::Ifd),
        raw.model.clone(),
    ); // Model
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0111, IfdType::Ifd),
        IfdValue::Offsets(Arc::new(thumb_data)),
    ); // StripOffsets
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0112, IfdType::Ifd),
        orientation,
    ); // Orientation
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0115, IfdType::Ifd),
        3u16,
    ); // SamplesPerPixel
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0116, IfdType::Ifd),
        thumb_h as u32,
    ); // RowsPerStrip
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0117, IfdType::Ifd),
        (thumb_w * thumb_h * 3) as u32,
    ); // StripByteCounts
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x011c, IfdType::Ifd),
        1u16,
    ); // PlanarConfiguration
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x0131, IfdType::Ifd),
        "raw-to-dng-converter".to_string(),
    ); // Software
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0x014a, IfdType::Ifd),
        IfdValue::List(vec![IfdValue::Ifd(sub_ifd)]),
    ); // SubIFDs
    ifd0.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xa001), 65535u16); // ColorSpace: Uncalibrated
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc612, IfdType::Ifd),
        &[1u8, 4, 0, 0] as &[u8],
    ); // DNGVersion
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc613, IfdType::Ifd),
        &[1u8, 1, 0, 0] as &[u8],
    ); // DNGBackwardVersion
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc614, IfdType::Ifd),
        format!("Sony {}", raw.model),
    ); // UniqueCameraModel
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc621, IfdType::Ifd),
        matrix_list,
    ); // ColorMatrix1
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc627, IfdType::Ifd),
        IfdValue::List(vec![
            IfdValue::Rational(1, 1),
            IfdValue::Rational(1, 1),
            IfdValue::Rational(1, 1),
        ]),
    ); // AnalogBalance
    if !neutral.is_empty() {
        ifd0.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0xc628, IfdType::Ifd),
            IfdValue::List(neutral),
        ); // AsShotNeutral
    }
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc62a, IfdType::Ifd),
        IfdValue::SRational(0, 100),
    ); // BaselineExposure
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc62b, IfdType::Ifd),
        IfdValue::Rational(100, 100),
    ); // BaselineNoise
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc62c, IfdType::Ifd),
        IfdValue::Rational(100, 100),
    ); // BaselineSharpness
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc62e, IfdType::Ifd),
        IfdValue::Rational(100, 100),
    ); // LinearResponseLimit
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc633),
        IfdValue::Rational(1, 1),
    ); // ShadowScale
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc65a, IfdType::Ifd),
        21u16,
    ); // CalibrationIlluminant1
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc6f8),
        format!("Sony {}", raw.model),
    ); // ProfileName
    ifd0.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xc6fd), 0u32); // ProfileEmbedPolicy
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc65d),
        &[0u8; 16] as &[u8],
    ); // RawDataUniqueID

    // Write
    let mut output = Cursor::new(Vec::new());
    DngWriter::write_dng(&mut output, true, FileType::Dng, vec![ifd0])
        .map_err(|e| JsValue::from_str(&format!("Failed to write DNG: {:?}", e)))?;

    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_conversion() {
        let raw_data = fs::read("../samples/_DSC3731.ARW").expect("Failed to read ARW");
        let dng_data = convert_raw_to_dng(&raw_data, "arw").expect("Conversion failed");
        fs::write("../samples/_DSC3731_test.dng", dng_data).expect("Failed to write DNG");
    }
}
