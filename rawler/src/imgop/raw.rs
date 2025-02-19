// SPDX-License-Identifier: LGPL-2.1
// Copyright 2021 Daniel Vogelbacher <daniel@chaospixel.com>

use std::iter;
use crate::imgop::matrix::{multiply, normalize, pseudo_inverse};
use crate::imgop::sensor::bayer::superpixel::debayer_superpixel;
use crate::imgop::srgb::apply_gamma;
use crate::imgop::xyz::SRGB_TO_XYZ_D65;
use crate::imgop::{crop, Rect};
use super::sensor::bayer::BayerPattern;
use super::xyz::Illuminant;
use super::{Dim2, Result};


/// Conversion matrix for a specific illuminant
#[derive(Debug)]
pub struct ColorMatrix {
  pub illuminant: Illuminant,
  pub matrix: [[f32; 3]; 4],
}

/// Parameters for raw image development
#[derive(Debug)]
pub struct DevelopParams {
  pub width: usize,
  pub height: usize,
  pub color_matrices: Vec<ColorMatrix>,
  pub white_level: Vec<u16>,
  pub black_level: Vec<u16>,
  pub pattern: BayerPattern,
  pub wb_coeff: Vec<f32>,
  pub active_area: Option<Rect>,
  pub gamma: f32,
}

/// Develop a RAW image to sRGB
pub fn develop_raw_srgb(pixels: &Vec<u16>, params: &DevelopParams) -> Result<(Vec<f32>, Dim2)> {
  let black_level: [f32; 4] = match params.black_level.len() {
    1 => Ok(collect_array(iter::repeat(params.black_level[0] as f32))),
    4 => Ok(collect_array(params.black_level.iter().map(|p| *p as f32))),
    c @ _ => Err(format!("Black level sample count of {} is invalid", c)),
  }?;
  let white_level: [f32; 4] = match params.white_level.len() {
    1 => Ok(collect_array(iter::repeat(params.white_level[0] as f32))),
    4 => Ok(collect_array(params.white_level.iter().map(|p| *p as f32))),
    c @ _ => Err(format!("White level sample count of {} is invalid", c)),
  }?;
  let wb_coeff: [f32; 4] = match params.wb_coeff.len() {
    1 => Ok(collect_array(iter::repeat(params.wb_coeff[0]))),
    4 => Ok(collect_array(params.wb_coeff.iter().map(|p| *p))),
    3 => Ok(match params.pattern {
      BayerPattern::RGGB | BayerPattern::BGGR => [params.wb_coeff[0], params.wb_coeff[1], params.wb_coeff[1], params.wb_coeff[2]],
      BayerPattern::GBRG | BayerPattern::GRBG => [params.wb_coeff[0], params.wb_coeff[1], params.wb_coeff[2], params.wb_coeff[0]],
      //BayerPattern::ERBG => todo!(),
      //BayerPattern::RGEB => todo!(),
    }),
    c @ _ => Err(format!("AsShot wb_coeff sample count of {} is invalid", c)),
  }?;

  //Color Space Conversion
  let xyz2cam = params
    .color_matrices
    .iter()
    .filter(|m| m.illuminant == Illuminant::D65)
    .next()
    .ok_or("Illuminant matrix D65 not found")?
    .matrix;

  let rgb2cam = normalize(multiply(&xyz2cam, &SRGB_TO_XYZ_D65));
  let cam2rgb = pseudo_inverse(rgb2cam);

  let active_area = params.active_area.unwrap();

  //eprintln!("cam2rgb: {:?}", cam2rgb);
  let cropped_pixels = crop(&pixels, Dim2::new(params.width, params.height), active_area);

  let (rgb, w, h) = debayer_superpixel(&cropped_pixels, params.pattern, active_area.d, &black_level, &white_level, &wb_coeff);

  // Convert to sRGB from XYZ
  let srgb: Vec<f32> = rgb
    .iter()
    .map(|c| (c[0], c[1], c[2]))
    .map(|(r, g, b)| {
      [
        cam2rgb[0][0] * r + cam2rgb[0][1] * g + cam2rgb[0][2] * b,
        cam2rgb[1][0] * r + cam2rgb[1][1] * g + cam2rgb[1][2] * b,
        cam2rgb[2][0] * r + cam2rgb[2][1] * g + cam2rgb[2][2] * b,
      ]
    })
    .flatten()
    .map(|p| apply_gamma(p, params.gamma))
    .collect();

  Ok((srgb, Dim2::new(w, h)))
}

/// Collect iterator into array
fn collect_array<T, I, const N: usize>(itr: I) -> [T; N]
where
  T: Default + Copy,
  I: IntoIterator<Item = T>,
{
  let mut res = [T::default(); N];
  for (it, elem) in res.iter_mut().zip(itr) {
    *it = elem
  }

  res
}

/// Calculate the multiplicative invert of an array
/// (sum of each row equals to 1.0)
pub fn mul_invert_array<const N: usize>(a: &[f32; N]) -> [f32; N] {
  let mut b = [f32::default(); N];
  b.iter_mut().zip(a.iter()).for_each(|(x, y)| *x = 1.0 / y);
  b
}
