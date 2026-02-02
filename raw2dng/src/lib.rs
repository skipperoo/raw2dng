use dng::ifd::{Ifd, IfdValue};
use dng::tags::{IfdType, MaybeKnownIfdFieldDescriptor};
use dng::{DngWriter, FileType};
use flate2::write::ZlibEncoder;
use flate2::Compression as FlateCompression;
use rawler::decoders::{RawDecodeParams, RawLoader as RawlerLoader};
use rawler::formats::tiff::reader::{GenericTiffReader, TiffReader};
use rawler::formats::tiff::Value as RawlerValue;
use rawler::imgop::xyz::{Illuminant, XYZ_TO_SRGB_D65};
use rawler::rawsource::RawSource;
use rawloader::{Orientation as RawOrientation, RawImageData, RawLoader};
use std::io::{Cursor, Write};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn rawler_to_dng_value(v: &RawlerValue) -> Option<IfdValue> {
    match v {
        RawlerValue::Byte(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Byte(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Byte(x)).collect(),
                ))
            }
        }
        RawlerValue::Short(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Short(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Short(x)).collect(),
                ))
            }
        }
        RawlerValue::Long(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Long(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Long(x)).collect(),
                ))
            }
        }
        RawlerValue::Rational(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Rational(vec[0].n, vec[0].d))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|x| IfdValue::Rational(x.n, x.d)).collect(),
                ))
            }
        }
        RawlerValue::SByte(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::SByte(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::SByte(x)).collect(),
                ))
            }
        }
        RawlerValue::SShort(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::SShort(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::SShort(x)).collect(),
                ))
            }
        }
        RawlerValue::SLong(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::SLong(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::SLong(x)).collect(),
                ))
            }
        }
        RawlerValue::SRational(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::SRational(vec[0].n, vec[0].d))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|x| IfdValue::SRational(x.n, x.d)).collect(),
                ))
            }
        }
        RawlerValue::Float(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Float(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Float(x)).collect(),
                ))
            }
        }
        RawlerValue::Double(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Double(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Double(x)).collect(),
                ))
            }
        }
        RawlerValue::Ascii(tiff_ascii) => {
            if let Some(s) = tiff_ascii.strings().first() {
                Some(IfdValue::Ascii(s.clone()))
            } else {
                None
            }
        }
        RawlerValue::Undefined(vec) => {
            if vec.len() == 1 {
                Some(IfdValue::Undefined(vec[0]))
            } else {
                Some(IfdValue::List(
                    vec.iter().map(|&x| IfdValue::Undefined(x)).collect(),
                ))
            }
        }
        RawlerValue::Unknown(_, _) => None,
    }
}

fn transfer_tags(src: &rawler::formats::tiff::IFD, dst: &mut Ifd, blacklist: &[u16]) {
    for (&tag, entry) in src.entries() {
        if blacklist.contains(&tag) {
            continue;
        }
        match tag {
            0x0111 | 0x0117 | 0x0144 | 0x0145 | 0x014a | 0x8769 | 0x8825 | 0x0100 | 0x0101
            | 0x0102 | 0x0103 | 0x0106 | 0x0115 | 0x0116 | 0x011c | 0x0153 => continue,
            _ => {}
        }
        if let Some(val) = rawler_to_dng_value(&entry.value) {
            dst.insert(
                MaybeKnownIfdFieldDescriptor::from_number(tag, dst.get_type()),
                val,
            );
        }
    }
}

fn invert_3x3(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);

    if det.abs() < 1e-10 {
        return None;
    }

    let inv_det = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ])
}

fn mat_mul_3x3(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut res = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                res[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    res
}

pub fn convert_raw_to_dng_inner(input: &[u8], _format: &str) -> Result<Vec<u8>, String> {
    let loader = RawLoader::new();
    let raw = loader
        .decode(&mut Cursor::new(input), false)
        .map_err(|e| format!("Failed to parse RAW: {:?}", e))?;

    let source = RawSource::new_from_slice(input);
    let rawler_loader = RawlerLoader::new();
    let decoder = rawler_loader
        .get_decoder(&source)
        .map_err(|e| format!("Rawler failed to get decoder: {:?}", e))?;
    let metadata = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .map_err(|e| format!("Rawler failed to get metadata: {:?}", e))?;
    let exif = metadata.exif;

    // Get full RawImage info (for camera color matrices) without full decode
    let rawler_raw = rawler::decode_dummy(&source)
        .map_err(|e| format!("Rawler failed to get camera info: {:?}", e))?;
    let camera = rawler_raw.camera;

    let tiff_reader = GenericTiffReader::new_with_buffer(input, 0, 0, None).ok();

    let orientation = match raw.orientation {
        RawOrientation::Normal => 1u16,
        RawOrientation::Rotate90 => 6u16,
        RawOrientation::Rotate180 => 3u16,
        RawOrientation::Rotate270 => 8u16,
        _ => 1u16,
    };

    // --- Color Calibration from Rawler ---
    let mut matrix1 = None;
    let mut matrix2 = None;
    let mut illu1 = Illuminant::Unknown;
    let mut illu2 = Illuminant::Unknown;

    // Pick matrices
    if let Some(m) = camera
        .color_matrix
        .get(&Illuminant::D65)
        .or_else(|| camera.color_matrix.get(&Illuminant::D55))
    {
        matrix2 = Some(m.clone());
        illu2 = Illuminant::D65;
    }
    if let Some(m) = camera
        .color_matrix
        .get(&Illuminant::A)
        .or_else(|| camera.color_matrix.get(&Illuminant::Tungsten))
    {
        matrix1 = Some(m.clone());
        illu1 = Illuminant::A;
    }

    // Fallback if we only have one
    if matrix1.is_none() && matrix2.is_none() {
        if let Some((&illu, m)) = camera.color_matrix.iter().next() {
            matrix1 = Some(m.clone());
            illu1 = illu;
        }
    }

    // Default Sony-like fallback if still none
    let matrix1_final = matrix1.unwrap_or_else(|| {
        vec![
            0.5271, -0.0712, -0.0347, -0.6153, 1.3653, 0.2763, -0.1601, 0.2366, 0.7242,
        ]
    });
    let illu1_final = if illu1 == Illuminant::Unknown {
        Illuminant::D65
    } else {
        illu1
    };

    let cam_to_srgb = {
        let m = &matrix2.clone().unwrap_or_else(|| matrix1_final.clone());
        let xyz_to_cam = [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]];
        if let Some(cam_to_xyz) = invert_3x3(xyz_to_cam) {
            mat_mul_3x3(XYZ_TO_SRGB_D65, cam_to_xyz)
        } else {
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        }
    };

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

    // --- Thumbnail generation ---
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

                let r_lin = if r_cnt > 0 {
                    (r_sum / r_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                } * r_coeff;
                let g_lin = if g_cnt > 0 {
                    (g_sum / g_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                };
                let b_lin = if b_cnt > 0 {
                    (b_sum / b_cnt as f32 - black).max(0.0) / range
                } else {
                    0.0
                } * b_coeff;

                // Matrix transform to sRGB
                let rs = cam_to_srgb[0][0] * r_lin
                    + cam_to_srgb[0][1] * g_lin
                    + cam_to_srgb[0][2] * b_lin;
                let gs = cam_to_srgb[1][0] * r_lin
                    + cam_to_srgb[1][1] * g_lin
                    + cam_to_srgb[1][2] * b_lin;
                let bs = cam_to_srgb[2][0] * r_lin
                    + cam_to_srgb[2][1] * g_lin
                    + cam_to_srgb[2][2] * b_lin;

                let exposure = 1.5; // Slight boost
                let rs = (rs * exposure).max(0.0);
                let gs = (gs * exposure).max(0.0);
                let bs = (bs * exposure).max(0.0);

                // sRGB gamma
                let f = |x: f32| {
                    if x <= 0.0031308 {
                        12.92 * x
                    } else {
                        1.055 * x.powf(1.0 / 2.4) - 0.055
                    }
                };

                tdata.push((f(rs).min(1.0) * 255.0) as u8);
                tdata.push((f(gs).min(1.0) * 255.0) as u8);
                tdata.push((f(bs).min(1.0) * 255.0) as u8);
            }
        }
        (tdata, tw, th)
    } else {
        (vec![128u8; 16 * 16 * 3], 16, 16)
    };

    let byte_data = match &raw.data {
        RawImageData::Integer(data) => {
            let mut bytes = Vec::with_capacity(data.len() * 2);
            for &x in data {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
            bytes
        }
        _ => return Err("Unsupported data".to_string()),
    };

    let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
    encoder
        .write_all(&byte_data)
        .map_err(|e| format!("Failed to compress: {:?}", e))?;
    let compressed_data = encoder
        .finish()
        .map_err(|e| format!("Failed to finish compress: {:?}", e))?;
    let data_len = compressed_data.len() as u32;

    let black_level = raw.blacklevels[0] as u32;
    let white_level = raw.whitelevels[0] as u32;

    let mut sub_ifd = Ifd::new(IfdType::Ifd);
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

    let mut exif_ifd = Ifd::new(IfdType::Exif);

    // Try to block copy EXIF tags from source
    if let Some(reader) = &tiff_reader {
        let src_root_ifd = reader.root_ifd();
        if let Some(src_exif_ifds) = src_root_ifd.sub_ifds().get(&0x8769) {
            if let Some(src_exif_ifd) = src_exif_ifds.first() {
                transfer_tags(src_exif_ifd, &mut exif_ifd, &[]);
            }
        }
    }

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

    let mut ifd0 = Ifd::new(IfdType::Ifd);

    // Try to block copy root tags and GPS from source
    if let Some(reader) = &tiff_reader {
        let src_root_ifd = reader.root_ifd();
        transfer_tags(src_root_ifd, &mut ifd0, &[]);

        if let Some(src_gps_ifds) = src_root_ifd.sub_ifds().get(&0x8825) {
            if let Some(src_gps_ifd) = src_gps_ifds.first() {
                let mut gps_ifd = Ifd::new(IfdType::GpsInfo);
                transfer_tags(src_gps_ifd, &mut gps_ifd, &[]);
                ifd0.insert(
                    MaybeKnownIfdFieldDescriptor::from_number(0x8825, IfdType::Ifd),
                    IfdValue::Ifd(gps_ifd),
                );
            }
        }
    }

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
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xa001, IfdType::Ifd),
        1u16,
    ); // ColorSpace: sRGB
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
        format!("{} {}", raw.make, raw.model),
    ); // UniqueCameraModel

    // Embed dynamic matrices
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc621, IfdType::Ifd),
        IfdValue::List(
            matrix1_final
                .iter()
                .map(|&v| IfdValue::SRational((v * 10000.0) as i32, 10000))
                .collect(),
        ),
    ); // ColorMatrix1
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::from_number(0xc65a, IfdType::Ifd),
        illu1_final as u16,
    ); // CalibrationIlluminant1

    if let Some(m2) = matrix2 {
        ifd0.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0xc622, IfdType::Ifd),
            IfdValue::List(
                m2.iter()
                    .map(|&v| IfdValue::SRational((v * 10000.0) as i32, 10000))
                    .collect(),
            ),
        ); // ColorMatrix2
        ifd0.insert(
            MaybeKnownIfdFieldDescriptor::from_number(0xc65b, IfdType::Ifd),
            illu2 as u16,
        ); // CalibrationIlluminant2
    }

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
        MaybeKnownIfdFieldDescriptor::Unknown(0xc6f8),
        format!("{} {}", raw.make, raw.model),
    ); // ProfileName
    ifd0.insert(MaybeKnownIfdFieldDescriptor::Unknown(0xc6fd), 0u32); // ProfileEmbedPolicy
    ifd0.insert(
        MaybeKnownIfdFieldDescriptor::Unknown(0xc65d),
        &[0u8; 16] as &[u8],
    ); // RawDataUniqueID

    let mut output = Cursor::new(Vec::new());
    DngWriter::write_dng(&mut output, true, FileType::Dng, vec![ifd0])
        .map_err(|e| format!("Failed to write DNG: {:?}", e))?;

    Ok(output.into_inner())
}

#[wasm_bindgen]
pub fn convert_raw_to_dng(input: &[u8], format: &str) -> Result<Vec<u8>, JsValue> {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    convert_raw_to_dng_inner(input, format).map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_conversion_arw() {
        let raw_data = fs::read("../samples/_DSC3731.ARW").expect("Failed to read ARW");
        let dng_data = convert_raw_to_dng_inner(&raw_data, "arw").expect("Conversion failed");
        fs::write("../samples/_DSC3731_test.dng", dng_data).expect("Failed to write DNG");
    }

    #[test]
    fn test_conversion_canon() {
        let raw_data =
            fs::read("../samples/sample-CR2-Image-File.cr2").expect("Failed to read CR2");
        let dng_data = convert_raw_to_dng_inner(&raw_data, "cr2").expect("Conversion failed");
        fs::write("../samples/canon_test.dng", dng_data).expect("Failed to write DNG");
    }
}
